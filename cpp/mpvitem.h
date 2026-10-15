#pragma once

#include <MpvQt/mpvabstractitem.h>
#include <QtQml/qqmlregistration.h>

// The embedded video surface: KDE's MpvQt item, configured to decode in hardware on the
// integrated GPU where there is one, so a hybrid laptop's discrete GPU can stay asleep.
class MpvItem : public MpvAbstractItem
{
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(QString mpvVersion READ mpvVersion NOTIFY mpvVersionChanged)
    Q_PROPERTY(double position READ position NOTIFY positionChanged)
    Q_PROPERTY(double duration READ duration NOTIFY durationChanged)
    Q_PROPERTY(bool paused READ paused WRITE setPaused NOTIFY pausedChanged)
    Q_PROPERTY(double speed READ speed WRITE setSpeed NOTIFY speedChanged)
    Q_PROPERTY(double volume READ volume WRITE setVolume NOTIFY volumeChanged)
    Q_PROPERTY(bool muted READ muted WRITE setMuted NOTIFY mutedChanged)
    // True while waiting for the network (startup or a stall).
    Q_PROPERTY(bool buffering READ buffering NOTIFY bufferingChanged)
    // Seconds of video buffered ahead of the playhead.
    Q_PROPERTY(double bufferedAhead READ bufferedAhead NOTIFY bufferedAheadChanged)
    // "vaapi" when the iGPU decodes; "no" means software decoding.
    Q_PROPERTY(QString hwdec READ hwdec NOTIFY hwdecChanged)
    Q_PROPERTY(bool loaded READ loaded NOTIFY loadedChanged)
    // Display width / height of the playing video (0.5625 for a vertical Short); 0 until known.
    Q_PROPERTY(double aspect READ aspect NOTIFY aspectChanged)
    // Whether the captions passed to loadStream are shown (mpv's sub-visibility).
    Q_PROPERTY(bool subtitlesVisible READ subtitlesVisible WRITE setSubtitlesVisible NOTIFY subtitlesVisibleChanged)
    // Where captions sit, in percent of the height (mpv's sub-pos; 100 is the bottom), so they
    // can move above the controls while those are showing.
    Q_PROPERTY(int subtitlePosition READ subtitlePosition WRITE setSubtitlePosition NOTIFY subtitlePositionChanged)

public:
    explicit MpvItem(QQuickItem *parent = nullptr);

    QString mpvVersion() const { return m_mpvVersion; }
    double position() const { return m_position; }
    double duration() const { return m_duration; }
    bool paused() const { return m_paused; }
    double speed() const { return m_speed; }
    double volume() const { return m_volume; }
    bool muted() const { return m_muted; }
    bool buffering() const { return m_buffering; }
    double bufferedAhead() const { return m_bufferedAhead; }
    QString hwdec() const { return m_hwdec; }
    bool loaded() const { return m_loaded; }
    double aspect() const { return m_aspect; }
    bool subtitlesVisible() const { return m_subtitlesVisible; }
    void setSubtitlesVisible(bool visible);
    int subtitlePosition() const { return m_subtitlePosition; }
    void setSubtitlePosition(int percent);

    void setPaused(bool paused);
    void setSpeed(double speed);
    void setVolume(double volume);
    void setMuted(bool muted);

    // Plays separate video and audio streams (audioUrl may be empty) from `start` seconds, with
    // captions from subtitleUrl when it isn't empty. `headers` is "Key: value" lines from
    // yt-dlp; they must accompany every request.
    Q_INVOKABLE void loadStream(const QString &videoUrl, const QString &audioUrl, const QString &headers, double start,
                                const QString &subtitleUrl);
    Q_INVOKABLE void stop();
    Q_INVOKABLE void seekTo(double seconds);
    Q_INVOKABLE void seekBy(double seconds);

Q_SIGNALS:
    void mpvVersionChanged();
    void positionChanged();
    void durationChanged();
    void pausedChanged();
    void speedChanged();
    void volumeChanged();
    void mutedChanged();
    void bufferingChanged();
    void bufferedAheadChanged();
    void hwdecChanged();
    void loadedChanged();
    void aspectChanged();
    void subtitlesVisibleChanged();
    void subtitlePositionChanged();
    // Playback reached the end (the last frame stays on screen).
    void finished();
    void loadFailed(const QString &reason);
    // A seek was requested; `target` is where it goes (seconds). For MPRIS's Seeked.
    void seeked(double target);

private:
    void onPropertyChanged(const QString &property, const QVariant &value);

    QString m_mpvVersion;
    double m_position = 0;
    double m_duration = 0;
    double m_aspect = 0;
    bool m_subtitlesVisible = true;
    int m_subtitlePosition = 100;
    // Added once the file has loaded (mpv's sub-add needs a loaded file).
    QString m_subtitleUrl;
    bool m_paused = false;
    double m_speed = 1;
    double m_volume = 100;
    bool m_muted = false;
    bool m_buffering = false;
    double m_bufferedAhead = 0;
    QString m_hwdec;
    bool m_loaded = false;
};
