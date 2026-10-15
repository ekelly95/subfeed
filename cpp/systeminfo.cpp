#include "systeminfo.h"

#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusConnectionInterface>
#include <QtDBus/QDBusInterface>
#include <QtDBus/QDBusReply>

#include <fcntl.h>
#include <unistd.h>
#include <va/va.h>
#include <va/va_drm.h>

#include <vector>

namespace
{
QStringList renderNodes()
{
    return QDir(QStringLiteral("/dev/dri")).entryList({QStringLiteral("renderD*")}, QDir::System);
}

bool isNvidia(const QString &node)
{
    QFile vendor(QStringLiteral("/sys/class/drm/%1/device/vendor").arg(node));
    return vendor.open(QIODevice::ReadOnly) && vendor.readAll().trimmed() == "0x10de";
}

// Codecs with a VLD (decode) entrypoint on a DRM render node. On hybrid laptops NVIDIA nodes are
// skipped without being opened: opening one wakes a runtime-suspended dGPU, and playback uses
// the integrated GPU anyway.
QStringList probeVaapiCodecs()
{
    QStringList codecs;
    QStringList nodes = renderNodes();
    QStringList others;
    for (const QString &node : nodes) {
        if (!isNvidia(node)) {
            others << node;
        }
    }
    if (!others.isEmpty()) {
        nodes = others;
    }
    for (const QString &node : nodes) {
        const QByteArray path = QFile::encodeName(QStringLiteral("/dev/dri/") + node);
        const int fd = ::open(path.constData(), O_RDWR | O_CLOEXEC);
        if (fd < 0) {
            continue;
        }
        VADisplay display = vaGetDisplayDRM(fd);
        // Keep libva quiet; failures here are expected on some nodes.
        vaSetErrorCallback(display, nullptr, nullptr);
        vaSetInfoCallback(display, nullptr, nullptr);
        int major = 0, minor = 0;
        if (display && vaInitialize(display, &major, &minor) == VA_STATUS_SUCCESS) {
            std::vector<VAProfile> profiles(vaMaxNumProfiles(display));
            int count = 0;
            if (vaQueryConfigProfiles(display, profiles.data(), &count) == VA_STATUS_SUCCESS) {
                for (int i = 0; i < count; ++i) {
                    std::vector<VAEntrypoint> entrypoints(vaMaxNumEntrypoints(display));
                    int n = 0;
                    if (vaQueryConfigEntrypoints(display, profiles[i], entrypoints.data(), &n) != VA_STATUS_SUCCESS) {
                        continue;
                    }
                    bool decodes = false;
                    for (int e = 0; e < n; ++e) {
                        decodes |= entrypoints[e] == VAEntrypointVLD;
                    }
                    if (!decodes) {
                        continue;
                    }
                    QString codec;
                    switch (profiles[i]) {
                    case VAProfileAV1Profile0:
                        codec = QStringLiteral("av1");
                        break;
                    case VAProfileVP9Profile0:
                        codec = QStringLiteral("vp9");
                        break;
                    case VAProfileH264Main:
                    case VAProfileH264High:
                    case VAProfileH264ConstrainedBaseline:
                        codec = QStringLiteral("h264");
                        break;
                    default:
                        break;
                    }
                    if (!codec.isEmpty() && !codecs.contains(codec)) {
                        codecs << codec;
                    }
                }
            }
            vaTerminate(display);
        }
        ::close(fd);
    }
    return codecs;
}

const QString upowerService = QStringLiteral("org.freedesktop.UPower");
const QString upowerPath = QStringLiteral("/org/freedesktop/UPower");
const QString propertiesIface = QStringLiteral("org.freedesktop.DBus.Properties");
}

SystemInfo::SystemInfo(QObject *parent)
    : QObject(parent)
    , m_codecs(probeVaapiCodecs())
{
    QDBusConnection bus = QDBusConnection::systemBus();
    // power-profiles-daemon: the new name first, then the old one.
    const auto registered = bus.interface() ? bus.interface()->registeredServiceNames().value() : QStringList();
    for (const QString &name : {QStringLiteral("org.freedesktop.UPower.PowerProfiles"), QStringLiteral("net.hadess.PowerProfiles")}) {
        if (registered.contains(name)) {
            m_profilesService = name;
            break;
        }
    }

    bus.connect(upowerService, upowerPath, propertiesIface, QStringLiteral("PropertiesChanged"), this,
                SLOT(onPropertiesChanged(QString, QVariantMap, QStringList)));
    if (!m_profilesService.isEmpty()) {
        const QString path = QLatin1Char('/') + QString(m_profilesService).replace(QLatin1Char('.'), QLatin1Char('/'));
        bus.connect(m_profilesService, path, propertiesIface, QStringLiteral("PropertiesChanged"), this,
                    SLOT(onPropertiesChanged(QString, QVariantMap, QStringList)));
    }
    readPower();
}

static bool hybridNvidia()
{
    bool nvidia = false, other = false;
    for (const QString &node : renderNodes()) {
        (isNvidia(node) ? nvidia : other) = true;
    }
    return nvidia && other;
}

QString SystemInfo::preferredHwdec()
{
    return hybridNvidia() ? QStringLiteral("vaapi") : QStringLiteral("vaapi,nvdec,auto-safe");
}

QString SystemInfo::preferredHwdecInterop()
{
    return hybridNvidia() ? QStringLiteral("vaapi") : QStringLiteral("auto");
}

void SystemInfo::onPropertiesChanged(const QString &, const QVariantMap &changed, const QStringList &)
{
    if (changed.contains(QStringLiteral("OnBattery")) || changed.contains(QStringLiteral("ActiveProfile"))) {
        readPower();
    }
}

void SystemInfo::readPower()
{
    QDBusConnection bus = QDBusConnection::systemBus();
    auto get = [&bus](const QString &service, const QString &path, const QString &iface, const QString &prop) {
        QDBusInterface props(service, path, propertiesIface, bus);
        QDBusReply<QVariant> reply = props.call(QStringLiteral("Get"), iface, prop);
        return reply.isValid() ? reply.value() : QVariant();
    };

    bool onBattery = get(upowerService, upowerPath, upowerService, QStringLiteral("OnBattery")).toBool();
    bool powerSaver = false;
    if (!m_profilesService.isEmpty()) {
        const QString path = QLatin1Char('/') + QString(m_profilesService).replace(QLatin1Char('.'), QLatin1Char('/'));
        powerSaver = get(m_profilesService, path, m_profilesService, QStringLiteral("ActiveProfile")).toString()
            == QLatin1String("power-saver");
    }
    if (onBattery != m_onBattery || powerSaver != m_powerSaver) {
        m_onBattery = onBattery;
        m_powerSaver = powerSaver;
        Q_EMIT powerChanged();
    }
}
