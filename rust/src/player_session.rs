//! `PlayerSession`: what the player page binds to. `open(id)` resolves streams and metadata with
//! yt-dlp on a background thread (or takes a prefetched lookup, see `lookup.rs`); QML then hands
//! the URLs to the mpv item.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        /// "idle", "loading", "ready" or "error".
        #[qproperty(QString, state)]
        #[qproperty(QString, video_id, cxx_name = "videoId")]
        #[qproperty(QString, title)]
        #[qproperty(QString, channel)]
        #[qproperty(QString, published)]
        #[qproperty(f64, duration)]
        #[qproperty(QString, description)]
        /// `[{"start_time": 0, "title": "…"}, …]`
        #[qproperty(QString, chapters_json, cxx_name = "chaptersJson")]
        /// "720p · AV1"
        #[qproperty(QString, quality)]
        /// The stream was chosen for low power (battery or power-saver).
        #[qproperty(bool, on_battery, cxx_name = "onBattery")]
        #[qproperty(QString, video_url, cxx_name = "videoUrl")]
        /// Empty when the video stream carries audio.
        #[qproperty(QString, audio_url, cxx_name = "audioUrl")]
        /// "Key: value" lines, for mpv's http-header-fields.
        #[qproperty(QString, http_headers, cxx_name = "httpHeaders")]
        /// Saved position to offer resuming from; 0 when there is nothing to resume.
        #[qproperty(f64, resume_from, cxx_name = "resumeFrom")]
        #[qproperty(QString, error_title, cxx_name = "errorTitle")]
        #[qproperty(QString, error_detail, cxx_name = "errorDetail")]
        /// Set when `adapt` switched streams, e.g. "Switched to 720p to save power".
        #[qproperty(QString, switch_note, cxx_name = "switchNote")]
        /// Captions for the viewer's language (see `stream::pick_captions`); "" when none.
        #[qproperty(QString, subtitle_url, cxx_name = "subtitleUrl")]
        /// "English (auto)"
        #[qproperty(QString, subtitle_label, cxx_name = "subtitleLabel")]
        /// Notes are being written (see `saveNotes`).
        #[qproperty(bool, saving_notes, cxx_name = "savingNotes")]
        /// SponsorBlock segments to skip: `[{"start", "end", "category", "label"}, …]`, filled in
        /// shortly after the video is ready ("[]" until then, or when there are none).
        #[qproperty(QString, skip_segments_json, cxx_name = "skipSegmentsJson")]
        type PlayerSession = super::PlayerSessionRust;
    }

    impl cxx_qt::Threading for PlayerSession {}

    extern "RustQt" {
        /// `codecs`: comma-separated codecs the GPU decodes (`SystemInfo.decodableCodecs`);
        /// `lang`: the viewer's locale ("en_US"), for captions.
        #[qinvokable]
        fn open(
            self: Pin<&mut PlayerSession>,
            video_id: &QString,
            on_battery: bool,
            power_saver: bool,
            codecs: &QString,
            lang: &QString,
        );
        /// Remembers where playback is (call every few seconds and on leaving).
        #[qinvokable]
        #[cxx_name = "savePosition"]
        fn save_position(self: Pin<&mut PlayerSession>, seconds: f64);
        #[qinvokable]
        #[cxx_name = "setWatched"]
        fn set_watched(self: Pin<&mut PlayerSession>, watched: bool);
        #[qinvokable]
        fn close(self: Pin<&mut PlayerSession>);
        /// Saves the open video as a Markdown note in `folder` (see `notes.rs`), in the background.
        #[qinvokable]
        #[cxx_name = "saveNotes"]
        fn save_notes(self: Pin<&mut PlayerSession>, folder: &QString);
        /// `openUrl` opens the note (in Obsidian when it's in a vault); `existed` means a note
        /// was already there and was left alone.
        #[qsignal]
        #[cxx_name = "notesSaved"]
        fn notes_saved(self: Pin<&mut PlayerSession>, path: QString, open_url: QString, existed: bool);
        #[qsignal]
        #[cxx_name = "notesFailed"]
        fn notes_failed(self: Pin<&mut PlayerSession>, reason: QString);
        /// The power state changed: picks streams again from the formats already fetched, and
        /// if they differ, switches (QML reloads at `resumeFrom`, which is set to `position`).
        #[qinvokable]
        fn adapt(self: Pin<&mut PlayerSession>, on_battery: bool, power_saver: bool, codecs: &QString, position: f64);
    }
}

use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

use crate::db::{self, Db};
use crate::lookup::Lookups;
use crate::stream::{self, Policy, Selection, VideoInfo};

pub struct PlayerSessionRust {
    state: QString,
    video_id: QString,
    title: QString,
    channel: QString,
    published: QString,
    duration: f64,
    description: QString,
    chapters_json: QString,
    quality: QString,
    on_battery: bool,
    video_url: QString,
    audio_url: QString,
    http_headers: QString,
    resume_from: f64,
    error_title: QString,
    error_detail: QString,
    switch_note: QString,
    subtitle_url: QString,
    subtitle_label: QString,
    saving_notes: bool,
    skip_segments_json: QString,

    db: Option<Db>,
    /// The viewer's locale, from the last `open`.
    lang: String,
    /// The last resolved video, kept so the stream can be switched without asking YouTube again.
    last: Option<(VideoInfo, Selection)>,
    /// Bumped on every open, so a slow resolve for an older video is dropped.
    generation: u64,
}

impl Default for PlayerSessionRust {
    fn default() -> Self {
        Self {
            state: QString::from("idle"),
            video_id: QString::default(),
            title: QString::default(),
            channel: QString::default(),
            published: QString::default(),
            duration: 0.0,
            description: QString::default(),
            chapters_json: QString::from("[]"),
            quality: QString::default(),
            on_battery: false,
            video_url: QString::default(),
            audio_url: QString::default(),
            http_headers: QString::default(),
            resume_from: 0.0,
            error_title: QString::default(),
            error_detail: QString::default(),
            switch_note: QString::default(),
            subtitle_url: QString::default(),
            subtitle_label: QString::default(),
            saving_notes: false,
            skip_segments_json: QString::from("[]"),
            db: None,
            lang: String::new(),
            last: None,
            generation: 0,
        }
    }
}

fn parse_codecs(codecs: &QString) -> Vec<String> {
    codecs.to_string().split(',').map(str::trim).filter(|c| !c.is_empty()).map(String::from).collect()
}

/// Seconds near either end aren't worth offering to resume.
fn resume_point(saved: f64, duration: Option<f64>) -> f64 {
    let duration = duration.unwrap_or(f64::MAX);
    if saved >= 30.0 && saved < duration - 30.0 { saved } else { 0.0 }
}

fn headers(sel: &Selection) -> String {
    sel.video.http_headers.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n")
}

/// (title, detail) for the error card.
fn describe_error(e: &anyhow::Error) -> (String, String) {
    let msg = format!("{e:#}");
    if msg.contains("a bot") {
        (
            "YouTube wants to check this isn’t a bot".into(),
            "This usually happens on VPN addresses. Disconnect the VPN, then retry. Updating yt-dlp can also fix it.".into(),
        )
    } else if msg.contains("premiere") {
        ("This premiere hasn’t started yet".into(), String::new())
    } else if msg.contains("Live streams") {
        ("This is a live stream".into(), "subfeed can play it once the stream has ended.".into())
    } else if msg.contains("live stream recording is not available") {
        ("This live stream wasn’t saved".into(), "The channel didn’t keep a recording, so there’s nothing to play.".into())
    } else {
        ("Couldn’t load this video".into(), msg)
    }
}

/// Failures a newer yt-dlp may fix: yt-dlp erred, and not because of the video itself, a bot
/// check, or a live stream.
fn may_need_update(e: &anyhow::Error) -> bool {
    let msg = format!("{e:#}");
    msg.contains("yt-dlp failed")
        && !["unavailable", "Private video", "members", "removed", "live stream", "premiere", "a bot"]
            .iter()
            .any(|s| msg.contains(s))
}

struct Resolved {
    info: VideoInfo,
    selection: Selection,
    on_battery: bool,
}

impl qobject::PlayerSession {
    fn db(mut self: Pin<&mut Self>) -> Option<&Db> {
        if self.db.is_none() {
            self.as_mut().rust_mut().db = Db::open(&db::default_path()).ok();
        }
        self.into_ref().get_ref().db.as_ref()
    }

    pub fn open(
        mut self: Pin<&mut Self>,
        video_id: &QString,
        on_battery: bool,
        power_saver: bool,
        codecs: &QString,
        lang: &QString,
    ) {
        let id = video_id.to_string();
        let decodable = parse_codecs(codecs);
        self.as_mut().rust_mut().lang = lang.to_string();
        self.as_mut().set_subtitle_url(QString::default());
        self.as_mut().set_subtitle_label(QString::default());
        self.as_mut().set_skip_segments_json(QString::from("[]"));
        self.as_mut().rust_mut().last = None;
        self.as_mut().set_switch_note(QString::default());
        let generation = self.generation + 1;
        self.as_mut().rust_mut().generation = generation;
        self.as_mut().set_video_id(video_id.clone());
        self.as_mut().set_video_url(QString::default());
        self.as_mut().set_audio_url(QString::default());
        self.as_mut().set_error_title(QString::default());
        self.as_mut().set_error_detail(QString::default());
        self.as_mut().set_chapters_json(QString::from("[]"));
        self.as_mut().set_description(QString::default());
        self.as_mut().set_state(QString::from("loading"));
        let saved = self.as_mut().db().and_then(|db| db.position(&id).ok()).unwrap_or(0.0);

        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let result = Lookups::global().get(&id).and_then(|info| {
                let selection = stream::select(&info, Policy::for_power(on_battery, power_saver), &decodable)?;
                Ok(Resolved { info, selection, on_battery: on_battery || power_saver })
            });
            // When yt-dlp itself failed, see whether a newer one is out (one request to GitHub).
            let update_note = match &result {
                Err(e) if may_need_update(e) => crate::ytdlp::after_failure_note(),
                _ => None,
            };
            let _ = qt.queue(move |mut s| {
                if s.generation != generation {
                    return;
                }
                match result {
                    Ok(r) => s.as_mut().apply(r, saved),
                    Err(e) => {
                        let (title, mut detail) = describe_error(&e);
                        if let Some(note) = update_note {
                            detail = format!("{note}\n\n{detail}");
                        }
                        s.as_mut().set_error_title(QString::from(&title));
                        s.as_mut().set_error_detail(QString::from(&detail));
                        s.as_mut().set_state(QString::from("error"));
                    }
                }
            });
        });
    }

    fn apply(mut self: Pin<&mut Self>, r: Resolved, saved: f64) {
        let info = &r.info;
        let chapters = serde_json::to_string(&info.chapters).unwrap_or_else(|_| "[]".into());
        if let Some(db) = self.as_mut().db() {
            let _ = db.set_details(
                &info.id,
                info.description.as_deref(),
                &chapters,
                info.duration.map(|d| d.round() as i64),
                info.timestamp,
            );
        }
        let published = info
            .timestamp
            .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
            .map(|t| t.with_timezone(&chrono::Local).format("%b %-d").to_string())
            .unwrap_or_default();
        self.as_mut().set_title(QString::from(&info.title));
        self.as_mut().set_channel(QString::from(info.channel.as_deref().unwrap_or("")));
        self.as_mut().set_published(QString::from(&published));
        self.as_mut().set_duration(info.duration.unwrap_or(0.0));
        self.as_mut().set_description(QString::from(info.description.as_deref().unwrap_or("")));
        self.as_mut().set_chapters_json(QString::from(&chapters));
        self.as_mut().set_quality(QString::from(&r.selection.label()));
        self.as_mut().set_on_battery(r.on_battery);
        let caption = stream::pick_captions(info, &self.lang);
        self.as_mut().set_subtitle_url(QString::from(caption.as_ref().map_or("", |c| c.url.as_str())));
        self.as_mut().set_subtitle_label(QString::from(caption.as_ref().map_or("", |c| c.label.as_str())));
        self.as_mut().set_http_headers(QString::from(&headers(&r.selection)));
        self.as_mut().set_resume_from(resume_point(saved, info.duration));
        self.as_mut().set_audio_url(QString::from(
            r.selection.audio.as_ref().and_then(|a| a.url.as_deref()).unwrap_or(""),
        ));
        // Last: QML starts playback when videoUrl changes.
        self.as_mut().set_video_url(QString::from(r.selection.video.url.as_deref().unwrap_or("")));
        let id = r.info.id.clone();
        self.as_mut().rust_mut().last = Some((r.info, r.selection));
        self.as_mut().set_state(QString::from("ready"));

        // SponsorBlock, off the playback path: a slow or failed answer just skips nothing.
        let (generation, qt) = (self.generation, self.qt_thread());
        std::thread::spawn(move || {
            let segments = crate::sponsorblock::fetch(&id).unwrap_or_default();
            if segments.is_empty() {
                return;
            }
            let json = serde_json::to_string(&segments).unwrap_or_else(|_| "[]".into());
            let _ = qt.queue(move |mut s| {
                if s.generation == generation {
                    s.as_mut().set_skip_segments_json(QString::from(&json));
                }
            });
        });
    }

    pub fn adapt(mut self: Pin<&mut Self>, on_battery: bool, power_saver: bool, codecs: &QString, position: f64) {
        let Some((info, current)) = self.last.clone() else { return };
        let Ok(next) = stream::select(&info, Policy::for_power(on_battery, power_saver), &parse_codecs(codecs)) else {
            return;
        };
        let low = on_battery || power_saver;
        self.as_mut().set_on_battery(low);
        if next.video.format_id == current.video.format_id {
            return;
        }
        let note = if low {
            format!("Switched to {} to save power", next.label())
        } else {
            format!("Plugged in: switched to {}", next.label())
        };
        self.as_mut().set_switch_note(QString::from(&note));
        self.as_mut().set_quality(QString::from(&next.label()));
        self.as_mut().set_resume_from(position);
        self.as_mut().set_http_headers(QString::from(&headers(&next)));
        self.as_mut().set_audio_url(QString::from(next.audio.as_ref().and_then(|a| a.url.as_deref()).unwrap_or("")));
        self.as_mut().set_video_url(QString::from(next.video.url.as_deref().unwrap_or("")));
        self.as_mut().rust_mut().last = Some((info, next));
    }

    pub fn save_position(mut self: Pin<&mut Self>, seconds: f64) {
        let id = self.video_id.to_string();
        if id.is_empty() {
            return;
        }
        let now = chrono::Utc::now().timestamp();
        if let Some(db) = self.as_mut().db() {
            let _ = db.set_position(&id, seconds, now);
        }
    }

    pub fn set_watched(mut self: Pin<&mut Self>, watched: bool) {
        let id = self.video_id.to_string();
        let now = chrono::Utc::now().timestamp();
        if let Some(db) = self.as_mut().db() {
            let _ = db.set_watched(&id, watched, now);
        }
    }

    pub fn save_notes(mut self: Pin<&mut Self>, folder: &QString) {
        let Some((info, _)) = self.last.clone() else {
            self.notes_failed(QString::from("Notes can be saved once the video has loaded."));
            return;
        };
        if self.saving_notes {
            return;
        }
        self.as_mut().set_saving_notes(true);
        let (folder, lang, qt) = (folder.to_string(), self.lang.clone(), self.qt_thread());
        std::thread::spawn(move || {
            let result = crate::notes::save(&info, std::path::Path::new(&folder), &lang);
            let _ = qt.queue(move |mut s| {
                s.as_mut().set_saving_notes(false);
                match result {
                    Ok(saved) => {
                        let url = crate::notes::open_url(&saved.path);
                        let path = QString::from(&saved.path.to_string_lossy().to_string());
                        s.notes_saved(path, QString::from(&url), saved.existed);
                    }
                    Err(e) => s.notes_failed(QString::from(&format!("Couldn’t save notes: {e:#}"))),
                }
            });
        });
    }

    pub fn close(mut self: Pin<&mut Self>) {
        let generation = self.generation + 1;
        self.as_mut().rust_mut().generation = generation;
        self.as_mut().rust_mut().last = None;
        self.as_mut().set_video_url(QString::default());
        self.as_mut().set_audio_url(QString::default());
        self.set_state(QString::from("idle"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_only_in_the_middle() {
        assert_eq!(resume_point(10.0, Some(600.0)), 0.0);
        assert_eq!(resume_point(300.0, Some(600.0)), 300.0);
        assert_eq!(resume_point(590.0, Some(600.0)), 0.0);
        assert_eq!(resume_point(300.0, None), 300.0);
    }

    #[test]
    fn only_yt_dlp_breakage_suggests_an_update() {
        assert!(may_need_update(&anyhow::anyhow!("yt-dlp failed: ERROR: [youtube] x: nsig extraction failed")));
        assert!(!may_need_update(&anyhow::anyhow!("yt-dlp failed: ERROR: [youtube] x: Video unavailable")));
        assert!(!may_need_update(&anyhow::anyhow!("Live streams aren't supported yet")));
    }

    #[test]
    fn bot_checks_get_a_plain_explanation() {
        let e = anyhow::anyhow!("YouTube asked yt-dlp to prove it isn't a bot. If the VPN is on…");
        assert!(describe_error(&e).0.contains("bot"));
        let e = anyhow::anyhow!("yt-dlp failed: Video unavailable");
        assert_eq!(describe_error(&e).0, "Couldn’t load this video");
        let e = anyhow::anyhow!("yt-dlp failed: ERROR: [youtube] x: This live stream recording is not available.");
        assert_eq!(describe_error(&e).0, "This live stream wasn’t saved");
    }
}
