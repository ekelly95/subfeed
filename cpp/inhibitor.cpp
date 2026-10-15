#include "inhibitor.h"

#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusInterface>
#include <QtDBus/QDBusObjectPath>
#include <QtDBus/QDBusReply>

void Inhibitor::setActive(bool active)
{
    if (m_active == active) {
        return;
    }
    m_active = active;
    QDBusConnection bus = QDBusConnection::sessionBus();
    const QString reason = QStringLiteral("Playing a video");

    if (active) {
        QDBusInterface portal(QStringLiteral("org.freedesktop.portal.Desktop"), QStringLiteral("/org/freedesktop/portal/desktop"),
                              QStringLiteral("org.freedesktop.portal.Inhibit"), bus);
        // Flags: 4 = suspend, 8 = idle (screen blanking / locking).
        QDBusReply<QDBusObjectPath> handle =
            portal.call(QStringLiteral("Inhibit"), QString(), uint(4 | 8), QVariantMap{{QStringLiteral("reason"), reason}});
        if (handle.isValid()) {
            m_portalRequest = handle.value().path();
        } else {
            QDBusInterface saver(QStringLiteral("org.freedesktop.ScreenSaver"), QStringLiteral("/org/freedesktop/ScreenSaver"),
                                 QStringLiteral("org.freedesktop.ScreenSaver"), bus);
            QDBusReply<uint> cookie = saver.call(QStringLiteral("Inhibit"), QStringLiteral("subfeed"), reason);
            m_screenSaverCookie = cookie.isValid() ? cookie.value() : 0;
        }
    } else {
        if (!m_portalRequest.isEmpty()) {
            QDBusInterface request(QStringLiteral("org.freedesktop.portal.Desktop"), m_portalRequest,
                                   QStringLiteral("org.freedesktop.portal.Request"), bus);
            request.call(QStringLiteral("Close"));
            m_portalRequest.clear();
        }
        if (m_screenSaverCookie) {
            QDBusInterface saver(QStringLiteral("org.freedesktop.ScreenSaver"), QStringLiteral("/org/freedesktop/ScreenSaver"),
                                 QStringLiteral("org.freedesktop.ScreenSaver"), bus);
            saver.call(QStringLiteral("UnInhibit"), m_screenSaverCookie);
            m_screenSaverCookie = 0;
        }
    }
    Q_EMIT activeChanged();
}
