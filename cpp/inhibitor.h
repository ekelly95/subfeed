#pragma once

#include <QtCore/QObject>
#include <QtQml/qqmlregistration.h>

// Keeps the screen awake while `active` (a video is playing). Uses the XDG desktop portal's
// Inhibit (works on every desktop and inside Flatpak), falling back to org.freedesktop.ScreenSaver.
class Inhibitor : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(bool active READ active WRITE setActive NOTIFY activeChanged)

public:
    using QObject::QObject;
    ~Inhibitor() override { setActive(false); }

    bool active() const { return m_active; }
    void setActive(bool active);

Q_SIGNALS:
    void activeChanged();

private:
    bool m_active = false;
    QString m_portalRequest;    // object path to Close
    uint m_screenSaverCookie = 0;
};
