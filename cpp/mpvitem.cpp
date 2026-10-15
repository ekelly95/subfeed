#include "mpvitem.h"

#include "systeminfo.h"

#include <MpvQt/MpvController>

MpvItem::MpvItem(QQuickItem *parent)
    : MpvAbstractItem(parent)
{
    // Hardware decoding, preferring the integrated GPU (see SystemInfo::preferredHwdec).
    setProperty(QStringLiteral("hwdec"), SystemInfo::preferredHwdec());
    setProperty(QStringLiteral("gpu-hwdec-interop"), SystemInfo::preferredHwdecInterop());

    // On hybrid NVIDIA laptops, initialising a PipeWire audio output can power on the dGPU (its
    // HDMI audio function may be the first audio card), defeating runtime suspend while browsing.
    // So there is no audio output until something is actually played; loadStream() switches
    // `ao` to pipewire right before loading a video.
    setProperty(QStringLiteral("ao"), QStringLiteral("null"));
    setProperty(QStringLiteral("mute"), true);

    // Keep the last frame at the end (the "finished" overlay sits on it), cache generously.
    setProperty(QStringLiteral("keep-open"), QStringLiteral("yes"));
    setProperty(QStringLiteral("cache"), QStringLiteral("yes"));
    setProperty(QStringLiteral("demuxer-max-bytes"), QStringLiteral("150MiB"));
    // Streams come from yt-dlp already resolved; mpv must not run its own ytdl hook.
    setProperty(QStringLiteral("ytdl"), false);

    connect(mpvController(), &MpvController::propertyChanged, this, &MpvItem::onPropertyChanged);
    connect(mpvController(), &MpvController::fileLoaded, this, [this]() {
        if (!m_subtitleUrl.isEmpty()) {
            command({QStringLiteral("sub-add"), m_subtitleUrl, QStringLiteral("select"), QStringLiteral("Captions")});
        }
        m_loaded = true;
        Q_EMIT loadedChanged();
    });
    connect(mpvController(), &MpvController::endFile, this, [this](const QString &reason) {
        if (reason == QLatin1String("error")) {
            Q_EMIT loadFailed(QStringLiteral("The stream stopped loading. YouTube links expire; try opening it again."));
        }
        m_loaded = false;
        Q_EMIT loadedChanged();
    });

    connect(this, &MpvAbstractItem::ready, this, [this]() {
        m_mpvVersion = getProperty(QStringLiteral("mpv-version")).toString();
        Q_EMIT mpvVersionChanged();
        observeProperty(QStringLiteral("time-pos"), MPV_FORMAT_DOUBLE);
        observeProperty(QStringLiteral("duration"), MPV_FORMAT_DOUBLE);
        observeProperty(QStringLiteral("pause"), MPV_FORMAT_FLAG);
        observeProperty(QStringLiteral("speed"), MPV_FORMAT_DOUBLE);
        observeProperty(QStringLiteral("volume"), MPV_FORMAT_DOUBLE);
        observeProperty(QStringLiteral("mute"), MPV_FORMAT_FLAG);
        observeProperty(QStringLiteral("paused-for-cache"), MPV_FORMAT_FLAG);
        observeProperty(QStringLiteral("demuxer-cache-duration"), MPV_FORMAT_DOUBLE);
        observeProperty(QStringLiteral("hwdec-current"), MPV_FORMAT_STRING);
        observeProperty(QStringLiteral("eof-reached"), MPV_FORMAT_FLAG);
        observeProperty(QStringLiteral("video-params/aspect"), MPV_FORMAT_DOUBLE);
        observeProperty(QStringLiteral("sub-visibility"), MPV_FORMAT_FLAG);
    });
}

void MpvItem::onPropertyChanged(const QString &property, const QVariant &value)
{
    auto update = [](auto &field, auto v, auto signal) {
        if (field != v) {
            field = v;
            signal();
        }
    };
    if (property == QLatin1String("time-pos")) {
        update(m_position, value.toDouble(), [this] { Q_EMIT positionChanged(); });
    } else if (property == QLatin1String("duration")) {
        update(m_duration, value.toDouble(), [this] { Q_EMIT durationChanged(); });
    } else if (property == QLatin1String("pause")) {
        update(m_paused, value.toBool(), [this] { Q_EMIT pausedChanged(); });
    } else if (property == QLatin1String("speed")) {
        update(m_speed, value.toDouble(), [this] { Q_EMIT speedChanged(); });
    } else if (property == QLatin1String("volume")) {
        update(m_volume, value.toDouble(), [this] { Q_EMIT volumeChanged(); });
    } else if (property == QLatin1String("mute")) {
        update(m_muted, value.toBool(), [this] { Q_EMIT mutedChanged(); });
    } else if (property == QLatin1String("paused-for-cache")) {
        update(m_buffering, value.toBool(), [this] { Q_EMIT bufferingChanged(); });
    } else if (property == QLatin1String("demuxer-cache-duration")) {
        update(m_bufferedAhead, value.toDouble(), [this] { Q_EMIT bufferedAheadChanged(); });
    } else if (property == QLatin1String("hwdec-current")) {
        update(m_hwdec, value.toString(), [this] { Q_EMIT hwdecChanged(); });
    } else if (property == QLatin1String("sub-visibility")) {
        update(m_subtitlesVisible, value.toBool(), [this] { Q_EMIT subtitlesVisibleChanged(); });
    } else if (property == QLatin1String("video-params/aspect")) {
        // Unavailable (no video) arrives as an invalid value, i.e. 0.
        update(m_aspect, value.toDouble(), [this] { Q_EMIT aspectChanged(); });
    } else if (property == QLatin1String("eof-reached")) {
        if (value.toBool()) {
            Q_EMIT finished();
        }
    }
}

void MpvItem::loadStream(const QString &videoUrl, const QString &audioUrl, const QString &headers, double start,
                         const QString &subtitleUrl)
{
    m_subtitleUrl = subtitleUrl;
    // Audio output only now (see the constructor). SUBFEED_TEST_MUTE keeps tests silent while
    // still exercising the real PipeWire path.
    setProperty(QStringLiteral("ao"), QStringLiteral("pipewire"));
    setProperty(QStringLiteral("mute"), qEnvironmentVariableIsSet("SUBFEED_TEST_MUTE"));

    QStringList fields;
    for (const QString &line : headers.split(QLatin1Char('\n'), Qt::SkipEmptyParts)) {
        if (line.startsWith(QLatin1String("User-Agent:"), Qt::CaseInsensitive)) {
            setProperty(QStringLiteral("user-agent"), line.section(QLatin1Char(':'), 1).trimmed());
        } else {
            fields << line;
        }
    }
    setProperty(QStringLiteral("http-header-fields"), fields);

    // Per-file options; %N% length-prefixes values so commas in URLs can't split them.
    QStringList options;
    if (!audioUrl.isEmpty()) {
        options << QStringLiteral("audio-file=%%1%%2").arg(audioUrl.size()).arg(audioUrl);
    }
    if (start > 0) {
        options << QStringLiteral("start=%1").arg(start);
    }
    setPaused(false);
    command({QStringLiteral("loadfile"), videoUrl, QStringLiteral("replace"), QStringLiteral("-1"), options.join(QLatin1Char(','))});
}

void MpvItem::stop()
{
    command({QStringLiteral("stop")});
    // Release the audio device so the dGPU's HDMI audio function can suspend again.
    setProperty(QStringLiteral("ao"), QStringLiteral("null"));
    m_position = 0;
    Q_EMIT positionChanged();
}

void MpvItem::seekTo(double seconds)
{
    command({QStringLiteral("seek"), QString::number(seconds), QStringLiteral("absolute+exact")});
    Q_EMIT seeked(seconds);
}

void MpvItem::seekBy(double seconds)
{
    command({QStringLiteral("seek"), QString::number(seconds), QStringLiteral("relative")});
    Q_EMIT seeked(qBound(0.0, m_position + seconds, m_duration > 0 ? m_duration : m_position + seconds));
}

void MpvItem::setPaused(bool paused)
{
    setProperty(QStringLiteral("pause"), paused);
}

void MpvItem::setSpeed(double speed)
{
    setProperty(QStringLiteral("speed"), qBound(0.25, speed, 4.0));
}

void MpvItem::setVolume(double volume)
{
    setProperty(QStringLiteral("volume"), qBound(0.0, volume, 130.0));
}

void MpvItem::setSubtitlesVisible(bool visible)
{
    setProperty(QStringLiteral("sub-visibility"), visible);
}

void MpvItem::setSubtitlePosition(int percent)
{
    percent = qBound(0, percent, 100);
    if (percent == m_subtitlePosition) {
        return;
    }
    m_subtitlePosition = percent;
    setProperty(QStringLiteral("sub-pos"), percent);
    Q_EMIT subtitlePositionChanged();
}

void MpvItem::setMuted(bool muted)
{
    setProperty(QStringLiteral("mute"), muted);
}
