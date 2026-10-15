//! `BackgroundRefresh`: lets the app offer the Flatpak's background refresh (the Background
//! portal; see `cli::background_on`). Outside the Flatpak the systemd timer does this job.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        /// Running as a Flatpak, where the background refresh goes through the portal.
        #[qproperty(bool, available)]
        #[qproperty(bool, enabled)]
        type BackgroundRefresh = super::BackgroundRefreshRust;
    }

    impl cxx_qt::Threading for BackgroundRefresh {}

    extern "RustQt" {
        /// Asks the desktop (it may show a prompt) and starts the background refresh.
        #[qinvokable]
        fn enable(self: Pin<&mut BackgroundRefresh>);
        #[qsignal]
        fn failed(self: Pin<&mut BackgroundRefresh>, reason: QString);
    }
}

use std::pin::Pin;

use cxx_qt::Threading;
use cxx_qt_lib::QString;

use crate::db::{self, Db};

pub struct BackgroundRefreshRust {
    available: bool,
    enabled: bool,
}

impl Default for BackgroundRefreshRust {
    fn default() -> Self {
        let available = std::path::Path::new("/.flatpak-info").exists();
        let enabled = available
            && Db::open(&db::default_path())
                .ok()
                .and_then(|db| db.setting("background").ok().flatten())
                .is_some_and(|v| v == "on");
        Self { available, enabled }
    }
}

impl qobject::BackgroundRefresh {
    pub fn enable(self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let result = crate::cli::background_on();
            let _ = qt.queue(move |mut o| match result {
                Ok(()) => o.as_mut().set_enabled(true),
                Err(e) => o.failed(QString::from(&format!("Background refresh is off: {e:#}"))),
            });
        });
    }
}
