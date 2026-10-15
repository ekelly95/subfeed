#pragma once

#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QVariantMap>
#include <QtDBus/QDBusAbstractAdaptor>
#include <QtDBus/QDBusObjectPath>
#include <QtQml/qqmlregistration.h>

#include "mpvitem.h"

// MPRIS (org.mpris.MediaPlayer2): media keys and any desktop's media widget control playback.
// The service is on the bus only while `active` (a video is open), so idle subfeed doesn't
// show up as a stopped player.
class Mpris : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(MpvItem *player READ player WRITE setPlayer NOTIFY changed)
    Q_PROPERTY(bool active READ active WRITE setActive NOTIFY changed)
    Q_PROPERTY(QString videoId MEMBER m_videoId WRITE setVideoId NOTIFY changed)
    Q_PROPERTY(QString title MEMBER m_title WRITE setTitle NOTIFY changed)
    Q_PROPERTY(QString artist MEMBER m_artist WRITE setArtist NOTIFY changed)
    Q_PROPERTY(QString artUrl MEMBER m_artUrl WRITE setArtUrl NOTIFY changed)
    Q_PROPERTY(bool canGoNext MEMBER m_canGoNext WRITE setCanGoNext NOTIFY changed)

public:
    explicit Mpris(QObject *parent = nullptr);
    ~Mpris() override;

    MpvItem *player() const { return m_player; }
    void setPlayer(MpvItem *player);
    bool active() const { return m_active; }
    void setActive(bool active);
    void setVideoId(const QString &v);
    void setTitle(const QString &v);
    void setArtist(const QString &v);
    void setArtUrl(const QString &v);
    void setCanGoNext(bool v);

    // Used by the D-Bus adaptors.
    QString playbackStatus() const;
    QVariantMap metadata() const;
    qlonglong positionUs() const;
    double rate() const;
    void setRate(double rate);
    double volume() const;
    void setVolume(double volume);
    bool canGoNext() const { return m_canGoNext; }
    bool canControl() const { return m_player != nullptr; }
    QDBusObjectPath trackPath() const;

    void playPause();
    void play();
    void pause();
    void stop();
    void seek(qlonglong offsetUs);
    void setPosition(const QDBusObjectPath &track, qlonglong positionUs);

Q_SIGNALS:
    void changed();
    void nextRequested();
    void previousRequested();
    void stopRequested();
    void raiseRequested();
    void quitRequested();

private:
    void playerPropertiesChanged(const QStringList &names);
    void sendPropertiesChanged(const QString &interface, const QVariantMap &changed);

    QPointer<MpvItem> m_player;
    bool m_active = false;
    QString m_videoId, m_title, m_artist, m_artUrl;
    bool m_canGoNext = false;
    QString m_serviceName;
    QObject *m_busObject = nullptr;
};

class MprisRootAdaptor : public QDBusAbstractAdaptor
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.mpris.MediaPlayer2")
    Q_PROPERTY(bool CanQuit READ yes)
    Q_PROPERTY(bool CanRaise READ yes)
    Q_PROPERTY(bool HasTrackList READ no)
    Q_PROPERTY(QString Identity READ identity)
    Q_PROPERTY(QString DesktopEntry READ desktopEntry)
    Q_PROPERTY(QStringList SupportedUriSchemes READ none)
    Q_PROPERTY(QStringList SupportedMimeTypes READ none)

public:
    MprisRootAdaptor(QObject *bus, Mpris *mpris);
    bool yes() const { return true; }
    bool no() const { return false; }
    QString identity() const { return QStringLiteral("subfeed"); }
    QString desktopEntry() const { return QStringLiteral("io.github.ekelly95.Subfeed"); }
    QStringList none() const { return {}; }

public Q_SLOTS:
    void Raise() { Q_EMIT m_mpris->raiseRequested(); }
    void Quit() { Q_EMIT m_mpris->quitRequested(); }

private:
    Mpris *m_mpris;
};

class MprisPlayerAdaptor : public QDBusAbstractAdaptor
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.mpris.MediaPlayer2.Player")
    Q_PROPERTY(QString PlaybackStatus READ playbackStatus)
    Q_PROPERTY(QString LoopStatus READ loopStatus)
    Q_PROPERTY(double Rate READ rate WRITE setRate)
    Q_PROPERTY(bool Shuffle READ no)
    Q_PROPERTY(QVariantMap Metadata READ metadata)
    Q_PROPERTY(double Volume READ volume WRITE setVolume)
    Q_PROPERTY(qlonglong Position READ position)
    Q_PROPERTY(double MinimumRate READ minRate)
    Q_PROPERTY(double MaximumRate READ maxRate)
    Q_PROPERTY(bool CanGoNext READ canGoNext)
    Q_PROPERTY(bool CanGoPrevious READ canControl)
    Q_PROPERTY(bool CanPlay READ canControl)
    Q_PROPERTY(bool CanPause READ canControl)
    Q_PROPERTY(bool CanSeek READ canControl)
    Q_PROPERTY(bool CanControl READ canControl)

public:
    MprisPlayerAdaptor(QObject *bus, Mpris *mpris);
    QString playbackStatus() const { return m_mpris->playbackStatus(); }
    QString loopStatus() const { return QStringLiteral("None"); }
    double rate() const { return m_mpris->rate(); }
    void setRate(double r) { m_mpris->setRate(r); }
    bool no() const { return false; }
    QVariantMap metadata() const { return m_mpris->metadata(); }
    double volume() const { return m_mpris->volume(); }
    void setVolume(double v) { m_mpris->setVolume(v); }
    qlonglong position() const { return m_mpris->positionUs(); }
    double minRate() const { return 0.25; }
    double maxRate() const { return 4.0; }
    bool canGoNext() const { return m_mpris->canGoNext(); }
    bool canControl() const { return m_mpris->canControl(); }

public Q_SLOTS:
    void Next() { Q_EMIT m_mpris->nextRequested(); }
    void Previous() { Q_EMIT m_mpris->previousRequested(); }
    void Pause() { m_mpris->pause(); }
    void PlayPause() { m_mpris->playPause(); }
    void Stop() { m_mpris->stop(); }
    void Play() { m_mpris->play(); }
    void Seek(qlonglong offset) { m_mpris->seek(offset); }
    void SetPosition(const QDBusObjectPath &track, qlonglong position) { m_mpris->setPosition(track, position); }
    void OpenUri(const QString &) { }

Q_SIGNALS:
    void Seeked(qlonglong position);

private:
    Mpris *m_mpris;
};
