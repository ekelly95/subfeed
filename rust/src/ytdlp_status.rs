//! `YtdlpStatus`: keeping yt-dlp, the part that goes stale when YouTube changes something,
//! current. In the Flatpak subfeed updates its own copy (asking GitHub at most daily). Elsewhere
//! the system owns yt-dlp, so only when it's over six weeks old does subfeed ask GitHub (at most
//! daily, remembered in the database) whether a newer one exists, and then says how to update.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        /// What to tell the user ("" when nothing): yt-dlp is missing, or a newer one is out.
        #[qproperty(QString, notice)]
        /// The command (or step) that updates it.
        #[qproperty(QString, hint)]
        /// The newer release the notice is about, so dismissing lasts until the next one.
        #[qproperty(QString, latest)]
        /// Set when subfeed updated its own yt-dlp (the Flatpak) to this version.
        #[qproperty(QString, updated)]
        type YtdlpStatus = super::YtdlpStatusRust;
    }

    impl cxx_qt::Threading for YtdlpStatus {}

    extern "RustQt" {
        /// Looks in the background; sets the properties when done.
        #[qinvokable]
        fn check(self: Pin<&mut YtdlpStatus>);
    }
}

use std::pin::Pin;

use cxx_qt::Threading;
use cxx_qt_lib::QString;

use crate::ytdlp;

/// Older than this, it's worth seeing whether a newer yt-dlp exists.
const STALE_DAYS: i64 = 42;

#[derive(Default)]
pub struct YtdlpStatusRust {
    notice: QString,
    hint: QString,
    latest: QString,
    updated: QString,
}

#[derive(Default)]
struct Status {
    notice: String,
    hint: String,
    latest: String,
    updated: String,
}

fn status(now: i64) -> Status {
    if ytdlp::managed_path().is_some() {
        return match ytdlp::auto_update(now, false) {
            Ok(Some(v)) => Status { updated: v, ..Status::default() },
            Ok(None) => Status::default(),
            Err(e) => Status {
                notice: format!("Couldn’t update yt-dlp ({e:#}). If videos stop playing, that’s why; subfeed tries again tomorrow."),
                ..Status::default()
            },
        };
    }
    let hint = ytdlp::update_hint(ytdlp::install());
    let Some(current) = ytdlp::version() else {
        return Status {
            notice: "yt-dlp isn't installed, so videos can't play. Install it from your distribution or github.com/yt-dlp/yt-dlp.".into(),
            ..Status::default()
        };
    };
    let today = chrono::DateTime::from_timestamp(now, 0).map(|t| t.date_naive());
    let age = ytdlp::version_date(&current).zip(today).map(|(d, t)| (t - d).num_days());
    if age.is_none_or(|days| days < STALE_DAYS) {
        return Status::default();
    }
    match ytdlp::latest_release_cached(now) {
        Some(latest) if ytdlp::is_newer(&latest, &current) => Status {
            notice: format!(
                "A newer yt-dlp ({latest}) is out; you have {current}. When videos stop playing, an old yt-dlp is usually why."
            ),
            hint,
            latest,
            updated: String::new(),
        },
        _ => Status::default(),
    }
}

impl qobject::YtdlpStatus {
    pub fn check(self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let s = status(chrono::Utc::now().timestamp());
            let _ = qt.queue(move |mut o| {
                o.as_mut().set_hint(QString::from(&s.hint));
                o.as_mut().set_latest(QString::from(&s.latest));
                o.as_mut().set_updated(QString::from(&s.updated));
                o.set_notice(QString::from(&s.notice));
            });
        });
    }
}
