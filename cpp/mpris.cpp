#include "mpris.h"
#include "mpvitem.h"

#include <QtCore/QCoreApplication>
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusMessage>

namespace
{
const QString objectPath = QStringLiteral("/org/mpris/MediaPlayer2");
const QString rootIface = QStringLiteral("org.mpris.MediaPlayer2");
const QString playerIface = QStringLiteral("org.mpris.MediaPlayer2.Player");
}

MprisRootAdaptor::MprisRootAdaptor(QObject *bus, Mpris *mpris)
    : QDBusAbstractAdaptor(bus)
    , m_mpris(mpris)
{
}

MprisPlayerAdaptor::MprisPlayerAdaptor(QObject *bus, Mpris *mpris)
    : QDBusAbstractAdaptor(bus)
    , m_mpris(mpris)
{
}

Mpris::Mpris(QObject *parent)
    : QObject(parent)
{
}

Mpris::~Mpris()
{
    setActive(false);
}

void Mpris::setPlayer(MpvItem *player)
{
    if (m_player == player) {
        return;
    }
    if (m_player) {
        disconnect(m_player, nullptr, this, nullptr);
    }
    m_player = player;
    if (m_player) {
        connect(m_player, &MpvItem::pausedChanged, this, [this] { playerPropertiesChanged({QStringLiteral("PlaybackStatus")}); });
        connect(m_player, &MpvItem::loadedChanged, this, [this] { playerPropertiesChanged({QStringLiteral("PlaybackStatus")}); });
        connect(m_player, &MpvItem::durationChanged, this, [this] { playerPropertiesChanged({QStringLiteral("Metadata")}); });
        connect(m_player, &MpvItem::speedChanged, this, [this] { playerPropertiesChanged({QStringLiteral("Rate")}); });
        connect(m_player, &MpvItem::volumeChanged, this, [this] { playerPropertiesChanged({QStringLiteral("Volume")}); });
        connect(m_player, &MpvItem::seeked, this, [this](double target) {
            if (m_busObject) {
                Q_EMIT m_busObject->findChild<MprisPlayerAdaptor *>()->Seeked(qlonglong(target * 1e6));
            }
        });
    }
    Q_EMIT changed();
}

void Mpris::setActive(bool active)
{
    if (m_active == active) {
        return;
    }
    m_active = active;
    QDBusConnection bus = QDBusConnection::sessionBus();
    if (active) {
        m_busObject = new QObject(this);
        new MprisRootAdaptor(m_busObject, this);
        new MprisPlayerAdaptor(m_busObject, this);
        bus.registerObject(objectPath, m_busObject);
        // A second subfeed (e.g. a test run) gets an instance-suffixed name, as the spec allows.
        m_serviceName = QStringLiteral("org.mpris.MediaPlayer2.subfeed");
        if (!bus.registerService(m_serviceName)) {
            m_serviceName += QStringLiteral(".instance%1").arg(QCoreApplication::applicationPid());
            bus.registerService(m_serviceName);
        }
    } else {
        if (!m_serviceName.isEmpty()) {
            bus.unregisterService(m_serviceName);
            m_serviceName.clear();
        }
        bus.unregisterObject(objectPath);
        delete m_busObject;
        m_busObject = nullptr;
    }
    Q_EMIT changed();
}

void Mpris::setVideoId(const QString &v)
{
    if (m_videoId != v) {
        m_videoId = v;
        playerPropertiesChanged({QStringLiteral("Metadata")});
        Q_EMIT changed();
    }
}

void Mpris::setTitle(const QString &v)
{
    if (m_title != v) {
        m_title = v;
        playerPropertiesChanged({QStringLiteral("Metadata")});
        Q_EMIT changed();
    }
}

void Mpris::setArtist(const QString &v)
{
    if (m_artist != v) {
        m_artist = v;
        playerPropertiesChanged({QStringLiteral("Metadata")});
        Q_EMIT changed();
    }
}

void Mpris::setArtUrl(const QString &v)
{
    if (m_artUrl != v) {
        m_artUrl = v;
        playerPropertiesChanged({QStringLiteral("Metadata")});
        Q_EMIT changed();
    }
}

void Mpris::setCanGoNext(bool v)
{
    if (m_canGoNext != v) {
        m_canGoNext = v;
        playerPropertiesChanged({QStringLiteral("CanGoNext")});
        Q_EMIT changed();
    }
}

QString Mpris::playbackStatus() const
{
    if (!m_player || !m_player->loaded()) {
        return QStringLiteral("Stopped");
    }
    return m_player->paused() ? QStringLiteral("Paused") : QStringLiteral("Playing");
}

QDBusObjectPath Mpris::trackPath() const
{
    // Object paths allow only [A-Za-z0-9_]; video ids also use '-'.
    QString id = m_videoId;
    id.replace(QLatin1Char('-'), QLatin1Char('_'));
    return QDBusObjectPath(QStringLiteral("/dev/subfeed/video/v") + (id.isEmpty() ? QStringLiteral("none") : id));
}

QVariantMap Mpris::metadata() const
{
    QVariantMap m;
    m.insert(QStringLiteral("mpris:trackid"), QVariant::fromValue(trackPath()));
    if (m_player && m_player->duration() > 0) {
        m.insert(QStringLiteral("mpris:length"), qlonglong(m_player->duration() * 1e6));
    }
    m.insert(QStringLiteral("xesam:title"), m_title);
    if (!m_artist.isEmpty()) {
        m.insert(QStringLiteral("xesam:artist"), QStringList{m_artist});
    }
    if (!m_artUrl.isEmpty()) {
        m.insert(QStringLiteral("mpris:artUrl"), m_artUrl);
    }
    if (!m_videoId.isEmpty()) {
        m.insert(QStringLiteral("xesam:url"), QStringLiteral("https://www.youtube.com/watch?v=") + m_videoId);
    }
    return m;
}

qlonglong Mpris::positionUs() const
{
    return m_player ? qlonglong(m_player->position() * 1e6) : 0;
}

double Mpris::rate() const
{
    return m_player ? m_player->speed() : 1.0;
}

void Mpris::setRate(double rate)
{
    if (m_player && rate > 0) {
        m_player->setSpeed(rate);
    }
}

double Mpris::volume() const
{
    return m_player ? m_player->volume() / 100.0 : 1.0;
}

void Mpris::setVolume(double volume)
{
    if (m_player) {
        m_player->setVolume(volume * 100.0);
    }
}

void Mpris::playPause()
{
    if (m_player) {
        m_player->setPaused(!m_player->paused());
    }
}

void Mpris::play()
{
    if (m_player) {
        m_player->setPaused(false);
    }
}

void Mpris::pause()
{
    if (m_player) {
        m_player->setPaused(true);
    }
}

void Mpris::stop()
{
    Q_EMIT stopRequested();
}

void Mpris::seek(qlonglong offsetUs)
{
    if (m_player) {
        m_player->seekBy(offsetUs / 1e6);
    }
}

void Mpris::setPosition(const QDBusObjectPath &track, qlonglong positionUs)
{
    if (m_player && track == trackPath() && positionUs >= 0) {
        m_player->seekTo(positionUs / 1e6);
    }
}

void Mpris::playerPropertiesChanged(const QStringList &names)
{
    if (!m_busObject) {
        return;
    }
    QVariantMap changed;
    for (const QString &name : names) {
        if (name == QLatin1String("PlaybackStatus")) {
            changed.insert(name, playbackStatus());
        } else if (name == QLatin1String("Metadata")) {
            changed.insert(name, metadata());
        } else if (name == QLatin1String("Rate")) {
            changed.insert(name, rate());
        } else if (name == QLatin1String("Volume")) {
            changed.insert(name, volume());
        } else if (name == QLatin1String("CanGoNext")) {
            changed.insert(name, m_canGoNext);
        }
    }
    sendPropertiesChanged(playerIface, changed);
}

void Mpris::sendPropertiesChanged(const QString &interface, const QVariantMap &changed)
{
    QDBusMessage signal = QDBusMessage::createSignal(objectPath, QStringLiteral("org.freedesktop.DBus.Properties"),
                                                     QStringLiteral("PropertiesChanged"));
    signal << interface << changed << QStringList();
    QDBusConnection::sessionBus().send(signal);
}
