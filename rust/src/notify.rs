//! Desktop notifications for new videos (org.freedesktop.Notifications), sent by
//! `subfeed --refresh --notify` from the background timer.
//!
//! One notification per refresh: "New from {channel}" for one video, "{n} new videos" for more,
//! with a Play action for the newest. Shorts, hidden videos and anything published more than
//! two days ago are left out, so coming back online after a week doesn't flood the desktop.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use crate::db::NotifyRow;
use crate::thumbs;

const MAX_AGE_S: i64 = 2 * 24 * 3600;
/// How long the background refresh waits for a click before exiting.
const WAIT_FOR_CLICK: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, PartialEq)]
pub struct Note {
    pub summary: String,
    pub body: String,
    /// The video the Play action opens (the newest).
    pub play_id: String,
}

/// Builds the notification, or None when nothing is worth one. `rows` are newest first.
pub fn compose(rows: &[NotifyRow], now: i64) -> Option<Note> {
    let news: Vec<&NotifyRow> = rows.iter().filter(|r| !r.is_short && now - r.published_at <= MAX_AGE_S).collect();
    let first = news.first()?;
    if news.len() == 1 {
        return Some(Note {
            summary: format!("New from {}", first.channel_title),
            body: first.title.clone(),
            play_id: first.id.clone(),
        });
    }
    let mut lines: Vec<String> = news.iter().take(3).map(|r| format!("{} — {}", r.channel_title, r.title)).collect();
    if news.len() > 3 {
        lines.push(format!("and {} more", news.len() - 3));
    }
    Some(Note { summary: format!("{} new videos", news.len()), body: lines.join("\n"), play_id: first.id.clone() })
}

/// True when NetworkManager says the connection is (probably) metered.
pub fn metered() -> bool {
    (|| -> Result<bool> {
        let conn = Connection::system()?;
        let nm = Proxy::new(&conn, "org.freedesktop.NetworkManager", "/org/freedesktop/NetworkManager", "org.freedesktop.NetworkManager")?;
        let m: u32 = nm.get_property("Metered")?;
        Ok(m == 1 || m == 3) // yes, guess-yes
    })()
    .unwrap_or(false)
}

/// Shows the notification and waits (up to 10 minutes) for a click. Play opens the video,
/// clicking the notification itself opens the app.
pub fn show_and_wait(note: &Note) -> Result<()> {
    let conn = Connection::session().context("connecting to the session bus")?;
    let proxy = Proxy::new(&conn, "org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications")?;

    let mut hints: HashMap<&str, Value> = HashMap::new();
    hints.insert("desktop-entry", Value::from("io.github.ekelly95.Subfeed"));
    hints.insert("category", Value::from("x-subfeed.new-videos"));
    let image = thumbs::path_for(&thumbs::cache_dir(), &note.play_id).filter(|p| p.is_file());
    if let Some(p) = &image {
        hints.insert("image-path", Value::from(p.to_string_lossy().into_owned()));
    }
    let actions = vec!["default", "Open subfeed", "play", "Play"];

    // Subscribe before sending, so a quick click isn't missed.
    let signals = proxy.receive_all_signals()?;
    let id: u32 = proxy
        .call("Notify", &("subfeed", 0u32, "video-display", note.summary.as_str(), note.body.as_str(), actions, hints, -1i32))
        .context("sending the notification")?;

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for msg in signals {
            if tx.send(msg).is_err() {
                break;
            }
        }
    });

    let mut token: Option<String> = None;
    let deadline = std::time::Instant::now() + WAIT_FOR_CLICK;
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        let Ok(msg) = rx.recv_timeout(left) else { break };
        let member = msg.header().member().map(|m| m.to_string()).unwrap_or_default();
        match member.as_str() {
            // Sent just before ActionInvoked: lets the opened window take focus on Wayland.
            "ActivationToken" => {
                if let Ok((nid, t)) = msg.body().deserialize::<(u32, String)>()
                    && nid == id
                {
                    token = Some(t);
                }
            }
            "ActionInvoked" => {
                if let Ok((nid, action)) = msg.body().deserialize::<(u32, String)>()
                    && nid == id
                {
                    let args: Vec<&str> = if action == "play" { vec!["--open", &note.play_id] } else { vec![] };
                    launch(&args, token.as_deref())?;
                    break;
                }
            }
            "NotificationClosed" => {
                if let Ok((nid, _reason)) = msg.body().deserialize::<(u32, u32)>()
                    && nid == id
                {
                    break;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// In the Flatpak: asks the Background portal to start `subfeed --background` at login
/// (`autostart`), or to stop doing so. The desktop may ask the user. Returns whether subfeed may
/// run in the background.
pub fn request_background(autostart: bool) -> Result<bool> {
    let conn = Connection::session().context("connecting to the session bus")?;
    let token = format!("subfeed{}", std::process::id());
    let sender = conn.unique_name().context("no bus name")?.trim_start_matches(':').replace('.', "_");
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let request = Proxy::new(&conn, "org.freedesktop.portal.Desktop", path.as_str(), "org.freedesktop.portal.Request")?;
    // Subscribe before asking, so the answer isn't missed.
    let mut answers = request.receive_signal("Response")?;
    let portal = Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Background",
    )?;
    let mut options: HashMap<&str, Value> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("reason", Value::from("Check for new videos while subfeed is closed"));
    options.insert("autostart", Value::from(autostart));
    options.insert("commandline", Value::from(vec!["subfeed", "--background"]));
    let _: OwnedObjectPath =
        portal.call("RequestBackground", &("", options)).context("asking the desktop's Background portal")?;
    let answer = answers.next().context("no answer from the Background portal")?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = answer.body().deserialize()?;
    if code != 0 {
        bail!("the request was declined");
    }
    Ok(results.get("background").and_then(|v| bool::try_from(v).ok()).unwrap_or(false))
}

/// In the Flatpak: starts `subfeed <args>` in a sandbox of its own (the Flatpak portal's Spawn),
/// so it outlives this process and doesn't keep this sandbox open. Returns its pid.
pub fn spawn_sandboxed(args: &[&str]) -> Result<u32> {
    let conn = Connection::session().context("connecting to the session bus")?;
    let portal =
        Proxy::new(&conn, "org.freedesktop.portal.Flatpak", "/org/freedesktop/portal/Flatpak", "org.freedesktop.portal.Flatpak")?;
    let nul = |s: &str| [s.as_bytes(), b"\0"].concat();
    let argv: Vec<Vec<u8>> = std::iter::once("subfeed").chain(args.iter().copied()).map(nul).collect();
    let fds: HashMap<u32, zbus::zvariant::OwnedFd> = HashMap::new();
    let envs: HashMap<String, String> = HashMap::new();
    let options: HashMap<&str, Value> = HashMap::new();
    let pid: u32 = portal.call("Spawn", &(nul("/"), argv, fds, envs, 0u32, options)).context("starting subfeed in its own sandbox")?;
    Ok(pid)
}

/// Starts subfeed (or hands over to the running window) without waiting for it.
fn launch(args: &[&str], activation_token: Option<&str>) -> Result<()> {
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.args(args);
    if let Some(t) = activation_token {
        cmd.env("XDG_ACTIVATION_TOKEN", t);
    }
    cmd.spawn().context("starting subfeed")?;
    Ok(())
}

/// The systemd user units for the background refresh.
pub fn timer_units(exe: &Path) -> (String, String) {
    let service = format!(
        "[Unit]\n\
         Description=Check YouTube subscriptions for new videos (subfeed)\n\
         # Only on mains power; metered connections are skipped by subfeed itself.\n\
         ConditionACPower=true\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart=\"{}\" --refresh --notify\n\
         # The refresh, then up to 10 minutes waiting for a click on the notification.\n\
         TimeoutStartSec=15min\n\
         Nice=10\n\
         IOSchedulingClass=idle\n",
        exe.display()
    );
    let timer = "[Unit]\n\
                 Description=Check YouTube subscriptions for new videos every 30 minutes (subfeed)\n\
                 \n\
                 [Timer]\n\
                 OnStartupSec=3min\n\
                 OnUnitActiveSec=30min\n\
                 RandomizedDelaySec=3min\n\
                 \n\
                 [Install]\n\
                 WantedBy=timers.target\n"
        .to_string();
    (service, timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, channel: &str, age_h: i64, short: bool) -> NotifyRow {
        NotifyRow {
            id: id.into(),
            title: format!("Title {id}"),
            channel_title: channel.into(),
            published_at: 1_000_000 - age_h * 3600,
            is_short: short,
        }
    }

    #[test]
    fn one_video() {
        let n = compose(&[row("aaaaaaaaaaa", "Fireship", 1, false)], 1_000_000).unwrap();
        assert_eq!(n.summary, "New from Fireship");
        assert_eq!(n.body, "Title aaaaaaaaaaa");
    }

    #[test]
    fn several_videos_list_three_then_count() {
        let rows: Vec<_> = (0..5).map(|i| row(&format!("v{i}"), "Chan", i, false)).collect();
        let n = compose(&rows, 1_000_000).unwrap();
        assert_eq!(n.summary, "5 new videos");
        assert_eq!(n.body.lines().count(), 4);
        assert!(n.body.ends_with("and 2 more"));
        assert_eq!(n.play_id, "v0");
    }

    #[test]
    fn shorts_and_old_videos_are_not_news() {
        let rows = [row("short", "A", 1, true), row("old", "B", 72, false)];
        assert_eq!(compose(&rows, 1_000_000), None);
        assert_eq!(compose(&[], 1_000_000), None);
    }

    #[test]
    fn units_run_this_binary_on_ac_only() {
        let (service, timer) = timer_units(Path::new("/opt/sub feed/subfeed"));
        assert!(service.contains("ExecStart=\"/opt/sub feed/subfeed\" --refresh --notify"));
        assert!(service.contains("ConditionACPower=true"));
        assert!(timer.contains("OnUnitActiveSec=30min"));
        assert!(timer.contains("WantedBy=timers.target"));
    }
}
