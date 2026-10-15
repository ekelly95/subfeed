#pragma once

#include <QtCore/QObject>

// subfeed's own D-Bus service (io.github.ekelly95.Subfeed, object /App). The first instance owns it;
// later launches (`subfeed`, `subfeed --open ID`, a notification's Play) hand over to it and exit.
class AppBus : public QObject
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "io.github.ekelly95.Subfeed.App")

public:
    static constexpr const char *service = "io.github.ekelly95.Subfeed";
    static constexpr const char *path = "/App";

    using QObject::QObject;

    // In a second instance: forward to the first. Returns false if there is no first instance.
    static bool forward(const QString &videoId);

public Q_SLOTS:
    // `token` is an XDG activation token, so the window may take focus on Wayland.
    Q_SCRIPTABLE void Activate(const QString &token);
    Q_SCRIPTABLE void Open(const QString &videoId, const QString &token);

Q_SIGNALS:
    void activateRequested();
    void openRequested(const QString &videoId);
};
