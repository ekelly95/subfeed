//! `Setup`: the first-run screen's backend. Imports a Google Takeout `subscriptions.csv`, or
//! follows channels one at a time by `@handle`, URL or id (resolved with yt-dlp in the
//! background).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        /// A channel is being looked up.
        #[qproperty(bool, busy)]
        /// What went wrong with the last import or add, in plain words; "" when fine.
        #[qproperty(QString, error)]
        /// Channels followed from this screen, newest first, as a JSON array of titles.
        #[qproperty(QString, added_json, cxx_name = "addedJson")]
        type Setup = super::SetupRust;
    }

    impl cxx_qt::Threading for Setup {}

    extern "RustQt" {
        /// Imports a Takeout export (a local path). Returns how many channels it lists, or -1
        /// on error (see `error`).
        #[qinvokable]
        #[cxx_name = "importTakeout"]
        fn import_takeout(self: Pin<&mut Setup>, path: &QString) -> i32;
        /// Follows a channel by `@handle`, URL or `UC…` id, in the background.
        #[qinvokable]
        #[cxx_name = "addChannel"]
        fn add_channel(self: Pin<&mut Setup>, input: &QString);
    }
}

use std::path::Path;
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

use crate::db::{self, Db};
use crate::{takeout, ytdlp};

pub struct SetupRust {
    busy: bool,
    error: QString,
    added_json: QString,
    added: Vec<String>,
}

impl Default for SetupRust {
    fn default() -> Self {
        Self { busy: false, error: QString::default(), added_json: QString::from("[]"), added: Vec::new() }
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Turns errors into what someone holding the wrong file needs to hear.
fn import_error(e: &anyhow::Error) -> String {
    let msg = format!("{e:#}");
    if msg.contains("not a channel id") || msg.contains("subscriptions.csv row") {
        "That doesn’t look like subscriptions.csv. In the Takeout download it’s in “YouTube and YouTube Music” → “subscriptions”.".into()
    } else if msg.contains("lists no channels") {
        "That file lists no channels. Is this the right YouTube account?".into()
    } else {
        format!("Couldn’t import: {msg}")
    }
}

impl qobject::Setup {
    pub fn import_takeout(self: Pin<&mut Self>, path: &QString) -> i32 {
        let path = path.to_string();
        let result = Db::open(&db::default_path()).and_then(|db| takeout::import(&db, Path::new(&path), now()));
        match result {
            Ok((_, total)) => {
                self.set_error(QString::default());
                total as i32
            }
            Err(e) => {
                self.set_error(QString::from(&import_error(&e)));
                -1
            }
        }
    }

    pub fn add_channel(mut self: Pin<&mut Self>, input: &QString) {
        let input = input.to_string().trim().to_string();
        if input.is_empty() || self.busy {
            return;
        }
        self.as_mut().set_busy(true);
        self.as_mut().set_error(QString::default());
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let result = ytdlp::resolve_channel(&input).and_then(|ch| {
                let db = Db::open(&db::default_path())?;
                db.add_channel(&ch, now())?;
                Ok(if ch.title.is_empty() { ch.id } else { ch.title })
            });
            let _ = qt.queue(move |mut s| {
                s.as_mut().set_busy(false);
                match result {
                    Ok(title) => {
                        s.as_mut().rust_mut().added.insert(0, title);
                        let json = serde_json::to_string(&s.added).unwrap_or_else(|_| "[]".into());
                        s.set_added_json(QString::from(&json));
                    }
                    Err(e) => {
                        let msg = format!("{e:#}");
                        let plain = if msg.contains("not a bot") || msg.contains("a bot") {
                            "YouTube wants to check this isn’t a bot. On a VPN, disconnect and try again.".to_string()
                        } else if msg.contains("isn't installed") {
                            msg
                        } else if msg.contains("yt-dlp failed") {
                            format!("Couldn’t look up “{input}” ({}). Check the name; if it’s right, update yt-dlp.", ytdlp::version_label())
                        } else {
                            format!("Couldn’t find “{input}”. Try the channel’s @handle or its URL.")
                        };
                        s.set_error(QString::from(&plain));
                    }
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_files_get_plain_messages() {
        let e = anyhow::anyhow!("row 2: \"abc\" is not a channel id; is this subscriptions.csv?");
        assert!(import_error(&e).contains("doesn’t look like subscriptions.csv"));
        let e = anyhow::anyhow!("/x/subscriptions.csv lists no channels");
        assert!(import_error(&e).contains("no channels"));
    }
}
