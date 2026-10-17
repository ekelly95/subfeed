#include <QtCore/QCommandLineParser>
#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QStandardPaths>
#include <QtCore/QThreadPool>
#include <QtCore/QTimer>
#include <QtDBus/QDBusConnection>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtQml/QQmlApplicationEngine>
#include <QtQuick/QQuickWindow>
#include <QtQuickControls2/QQuickStyle>

#include "appbus.h"

#include <clocale>
#include <memory>

// Rust: rust/src/cli.rs. Runs a headless command (--refresh, --import, ...) and returns its
// exit code, or -1 if there is none and the GUI should start.
extern "C" int subfeed_cli_main(int argc, const char *const *argv);

int main(int argc, char *argv[])
{
    // Before any GUI object, so `subfeed --refresh` works from a systemd timer with no display.
    if (const int code = subfeed_cli_main(argc, argv); code >= 0) {
        return code;
    }

    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("subfeed"));
    // Settings live in ~/.config/subfeed/subfeed.conf. Before the app ID changed they were under
    // ~/.config/subfeed.dev/; carry them over once.
    app.setOrganizationName(QStringLiteral("subfeed"));
    {
        const QString config = QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation);
        const QString oldFile = config + QStringLiteral("/subfeed.dev/subfeed.conf");
        const QString newFile = config + QStringLiteral("/subfeed/subfeed.conf");
        if (QFile::exists(oldFile) && !QFile::exists(newFile) && QDir().mkpath(config + QStringLiteral("/subfeed"))) {
            QFile::copy(oldFile, newFile);
        }
    }
    app.setApplicationVersion(QStringLiteral(SUBFEED_VERSION));
    // The app ID. Qt also registers it with the desktop portal, so the portal names subfeed
    // (e.g. on the screen-sleep block) even when it was started from a terminal.
    app.setDesktopFileName(QStringLiteral("io.github.ekelly95.Subfeed"));
    // Wayland desktops take the icon from the .desktop file; this covers X11 and uninstalled runs.
    app.setWindowIcon(QIcon::fromTheme(QStringLiteral("io.github.ekelly95.Subfeed"),
                                       QIcon(QStringLiteral(":/qt/qml/dev/subfeed/icons/io.github.ekelly95.Subfeed.svg"))));

    // libmpv requires the C numeric locale.
    std::setlocale(LC_NUMERIC, "C");

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral(
        "Distraction-free YouTube subscriptions\n\n"
        "Headless: --import <subscriptions.csv>, --add <channel>..., --remove <id>...,\n"
        "--refresh [--force] [--no-fallback] [--battery] [--notify], --list [N], --channels"));
    parser.addHelpOption();
    parser.addVersionOption();
    QCommandLineOption smoke(QStringLiteral("smoke"),
                             QStringLiteral("Open the window, wait for the player to initialise, then quit (for testing)."));
    parser.addOption(smoke);
    QCommandLineOption open(QStringLiteral("open"), QStringLiteral("Start playing this video id."), QStringLiteral("video-id"));
    parser.addOption(open);
    QCommandLineOption shorts(QStringLiteral("shorts"), QStringLiteral("Start in the Shorts view."));
    parser.addOption(shorts);
    QCommandLineOption manage(QStringLiteral("manage"), QStringLiteral("Start on the Channels page (add or remove channels)."));
    parser.addOption(manage);
    QCommandLineOption screenshot(QStringLiteral("screenshot"),
                                  QStringLiteral("Save a PNG of the window after a few seconds, then quit (for testing)."),
                                  QStringLiteral("file"));
    parser.addOption(screenshot);
    QCommandLineOption tour(QStringLiteral("tour"), QStringLiteral("Walk through the app by itself, then quit (for demo recordings)."));
    parser.addOption(tour);
    QCommandLineOption record(QStringLiteral("record"),
                              QStringLiteral("Save the window as numbered JPEG frames, 15 a second, into this folder (with --tour)."),
                              QStringLiteral("folder"));
    parser.addOption(record);
    parser.process(app);

    // One window: a second launch (or a notification's Play) hands over to the first and exits.
    // Test runs (--smoke, --screenshot) stay separate so they work beside a running subfeed.
    const bool testRun = parser.isSet(smoke) || parser.isSet(screenshot) || parser.isSet(tour);
    AppBus appBus;
    if (!testRun) {
        QDBusConnection bus = QDBusConnection::sessionBus();
        if (bus.isConnected() && !bus.registerService(QLatin1String(AppBus::service))) {
            if (AppBus::forward(parser.value(open))) {
                return 0;
            }
        }
        bus.registerObject(QLatin1String(AppBus::path), &appBus, QDBusConnection::ExportScriptableSlots);
    }

    // subfeed draws its own controls (qml/Theme.qml); the few stock ones (menus, scrollbars,
    // tooltips) use Qt's Basic style with a dark palette, so every desktop looks the same.
    if (qEnvironmentVariableIsEmpty("QT_QUICK_CONTROLS_STYLE")) {
        QQuickStyle::setStyle(QStringLiteral("Basic"));
    }

    QQmlApplicationEngine engine;
    engine.setInitialProperties({
        {QStringLiteral("smokeTest"), parser.isSet(smoke)},
        {QStringLiteral("openVideoId"), parser.value(open)},
        {QStringLiteral("startInShorts"), parser.isSet(shorts)},
        {QStringLiteral("startInChannels"), parser.isSet(manage)},
        {QStringLiteral("tour"), parser.isSet(tour)},
        // light|dark regardless of the desktop (KDE ignores an app's own request), for screenshots.
        {QStringLiteral("colorSchemeOverride"), qEnvironmentVariable("SUBFEED_COLOR_SCHEME")},
        {QStringLiteral("screenshotPath"), parser.value(screenshot)},
        // SUBFEED_SCREENSHOT_SCALE=2 renders --screenshot at twice the size (for the README).
        {QStringLiteral("screenshotScale"), qMax(1, qEnvironmentVariableIntValue("SUBFEED_SCREENSHOT_SCALE"))},
        {QStringLiteral("appBus"), QVariant::fromValue(static_cast<QObject *>(&appBus))},
    });
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     []() { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("dev.subfeed", "Main");

    // --record: the app films itself (no screen-capture permission, nothing else on screen).
    // Frames are grabbed on this thread and written as JPEG on worker threads.
    if (parser.isSet(record)) {
        auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
        const QString dir = parser.value(record);
        if (window && QDir().mkpath(dir)) {
            auto *frames = new QTimer(&app);
            auto count = std::make_shared<int>(0);
            QObject::connect(frames, &QTimer::timeout, &app, [window, dir, count]() {
                const QImage image = window->grabWindow();
                const QString path = QStringLiteral("%1/frame-%2.jpg").arg(dir).arg((*count)++, 5, 10, QLatin1Char('0'));
                QThreadPool::globalInstance()->start([image, path]() { image.save(path, "JPG", 92); });
            });
            frames->start(1000 / 15);
            QObject::connect(&app, &QCoreApplication::aboutToQuit, []() { QThreadPool::globalInstance()->waitForDone(); });
        }
    }

    if (parser.isSet(smoke) || parser.isSet(screenshot) || parser.isSet(tour)) {
        // Safety net: never hang a test run. QML timers run on the animation clock, which can lag
        // real time by a quarter while a video plays, so a player screenshot gets more room.
        const int limit = parser.isSet(tour) ? 150000 : !parser.isSet(screenshot) ? 10000 : parser.isSet(open) ? 90000 : 40000;
        QTimer::singleShot(limit, &app, []() { QCoreApplication::exit(2); });
    }
    return app.exec();
}
