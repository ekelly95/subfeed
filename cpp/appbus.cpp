#include "appbus.h"

#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusInterface>
#include <QtDBus/QDBusReply>

bool AppBus::forward(const QString &videoId)
{
    QDBusInterface first(QLatin1String(service), QLatin1String(path), QStringLiteral("io.github.ekelly95.Subfeed.App"));
    if (!first.isValid()) {
        return false;
    }
    const QString token = qEnvironmentVariable("XDG_ACTIVATION_TOKEN");
    const QDBusMessage reply = videoId.isEmpty() ? first.call(QStringLiteral("Activate"), token)
                                                 : first.call(QStringLiteral("Open"), videoId, token);
    return reply.type() == QDBusMessage::ReplyMessage;
}

static void useToken(const QString &token)
{
    // Qt's Wayland backend activates the next window with this token.
    if (!token.isEmpty()) {
        qputenv("XDG_ACTIVATION_TOKEN", token.toUtf8());
    }
}

void AppBus::Activate(const QString &token)
{
    useToken(token);
    Q_EMIT activateRequested();
}

void AppBus::Open(const QString &videoId, const QString &token)
{
    useToken(token);
    Q_EMIT openRequested(videoId);
}
