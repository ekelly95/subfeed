//! yt-dlp as a subprocess: resolving `@handles`/URLs to channel ids, and listing a channel's
//! `/videos` tab as a fallback when the RSS endpoint is down (it 404s for hours at a time).
//!
//! The `/videos` tab never contains Shorts. Its dates come from "3 days ago" text, so they are
//! marked approximate and replaced by RSS's exact ones whenever RSS works again.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::model::{Channel, VideoEntry, channel_url, is_channel_id, thumb_url};

/// The RSS feed lists 15 videos; ask the fallback for the same.
const TAB_ITEMS: &str = "1:15";

#[derive(Deserialize)]
struct Info {
    channel_id: Option<String>,
    channel: Option<String>,
    #[serde(default)]
    entries: Vec<TabEntry>,
}

#[derive(Deserialize)]
struct TabEntry {
    id: String,
    title: Option<String>,
    duration: Option<f64>,
    timestamp: Option<i64>,
    live_status: Option<String>,
}

#[derive(Debug)]
pub struct Tab {
    pub channel_title: Option<String>,
    pub entries: Vec<VideoEntry>,
}

pub(crate) fn run(args: &[&str]) -> Result<String> {
    let out = Command::new(binary())
        .args(["--no-warnings", "--ignore-config"])
        .args(args)
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => anyhow!("yt-dlp isn't installed. Install it from your distribution or github.com/yt-dlp/yt-dlp"),
            _ => anyhow!("running yt-dlp: {e}"),
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        if stderr.contains("Sign in to confirm") || stderr.contains("not a bot") {
            bail!("YouTube asked yt-dlp to prove it isn't a bot. If the VPN is on, disconnect it or try later.");
        }
        let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no output");
        bail!("yt-dlp failed: {last} ({}; if YouTube changed something, updating it often fixes this)", version_label());
    }
    String::from_utf8(out.stdout).context("yt-dlp printed invalid UTF-8")
}

fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// In the Flatpak, subfeed keeps its own yt-dlp here and updates it itself, since the bundled
/// one only changes with a new Flatpak build. Elsewhere yt-dlp belongs to the system.
pub fn managed_path() -> Option<PathBuf> {
    in_flatpak().then(|| dirs::data_dir().map(|d| d.join("subfeed").join("yt-dlp"))).flatten()
}

/// The yt-dlp to run: subfeed's own copy when it has one, else the one on PATH.
pub fn binary() -> PathBuf {
    managed_path().filter(|p| p.is_file()).unwrap_or_else(|| PathBuf::from("yt-dlp"))
}

static VERSION: Mutex<Option<Option<String>>> = Mutex::new(None);

/// yt-dlp's version ("2026.08.19"), asked once and only when needed: it's what goes stale
/// when YouTube changes something.
pub fn version() -> Option<String> {
    let mut cached = VERSION.lock().unwrap_or_else(|e| e.into_inner());
    cached
        .get_or_insert_with(|| {
            let out = Command::new(binary()).arg("--version").output().ok()?;
            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
            (out.status.success() && !v.is_empty()).then_some(v)
        })
        .clone()
}

/// The newest release, from the database if GitHub was asked within a day.
pub fn latest_release_cached(now: i64) -> Option<String> {
    const RECHECK_S: i64 = 24 * 60 * 60;
    let db = crate::db::Db::open(&crate::db::default_path()).ok()?;
    let checked = db.setting("ytdlp_latest_checked").ok()?.and_then(|t| t.parse::<i64>().ok());
    let cached = db.setting("ytdlp_latest").ok()?;
    if let (Some(t), Some(v)) = (checked, &cached)
        && now - t < RECHECK_S
    {
        return Some(v.clone());
    }
    let _ = db.set_setting("ytdlp_latest_checked", &now.to_string());
    let latest = latest_release().ok()?;
    let _ = db.set_setting("ytdlp_latest", &latest);
    Some(latest)
}

/// Downloads yt-dlp's official release `tag` into `managed_path`, after checking it against the
/// checksums yt-dlp publishes and that it runs and reports that version.
pub fn install_release(tag: &str) -> Result<()> {
    let dest = managed_path().context("only the Flatpak keeps its own yt-dlp")?;
    let dir = dest.parent().context("no folder for yt-dlp")?;
    std::fs::create_dir_all(dir)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .user_agent(concat!("subfeed/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let base = format!("https://github.com/yt-dlp/yt-dlp/releases/download/{tag}");
    let sums = agent.get(format!("{base}/SHA2-256SUMS")).call()?.body_mut().read_to_string()?;
    let expected = sums
        .lines()
        .find_map(|l| l.strip_suffix("  yt-dlp"))
        .context("yt-dlp's release lists no checksum for its zipapp")?
        .to_string();
    let bytes = agent.get(format!("{base}/yt-dlp")).call()?.body_mut().with_config().limit(64 << 20).read_to_vec()?;

    let tmp = dir.join("yt-dlp.download");
    std::fs::write(&tmp, &bytes)?;
    let check = (|| -> Result<()> {
        let out = Command::new("sha256sum").arg(&tmp).output().context("running sha256sum")?;
        let actual = String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap_or("").to_string();
        if actual != expected {
            bail!("the download doesn't match yt-dlp's published checksum");
        }
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        let out = Command::new(&tmp).arg("--version").output().context("running the new yt-dlp")?;
        let reported = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !out.status.success() || reported != tag {
            bail!("the new yt-dlp reports {reported:?}, not {tag}");
        }
        Ok(())
    })();
    if let Err(e) = check {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, &dest)?;
    *VERSION.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(())
}

/// In the Flatpak: installs a newer yt-dlp when there is one. GitHub is asked at most daily,
/// unless `now_please` (after a playback failure). Returns the version installed, if any.
pub fn auto_update(now: i64, now_please: bool) -> Result<Option<String>> {
    if managed_path().is_none() {
        return Ok(None);
    }
    let latest = if now_please { Some(latest_release()?) } else { latest_release_cached(now) };
    let Some(latest) = latest else { return Ok(None) };
    if !is_newer(&latest, &version().unwrap_or_default()) {
        return Ok(None);
    }
    install_release(&latest)?;
    Ok(Some(latest))
}

/// After a playback failure yt-dlp may be to blame: in the Flatpak, update it now and say so;
/// elsewhere, say whether a newer one is out.
pub fn after_failure_note() -> Option<String> {
    if managed_path().is_some() {
        let now = chrono::Utc::now().timestamp();
        return match auto_update(now, true) {
            Ok(Some(v)) => Some(format!("yt-dlp was out of date, so subfeed updated it to {v}. Press Retry.")),
            _ => None,
        };
    }
    outdated_note()
}

/// The release date in a version like "2026.08.19" (or "2026.08.19.1").
pub fn version_date(version: &str) -> Option<chrono::NaiveDate> {
    let mut parts = version.split('.').map(|p| p.parse::<u32>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    chrono::NaiveDate::from_ymd_opt(y as i32, m, d)
}

/// Whether `latest` is a newer release than `current` ("2026.10.02" > "2026.08.19").
pub fn is_newer(latest: &str, current: &str) -> bool {
    let nums = |v: &str| v.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    nums(latest) > nums(current)
}

/// How yt-dlp got here, which decides how to update it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Install {
    /// Bundled in subfeed's Flatpak: updating subfeed updates it.
    Flatpak,
    /// From the system's package manager (under /usr).
    System,
    /// Installed for this user (pip, pipx, or the standalone binary).
    User,
}

pub fn install() -> Install {
    if Path::new("/.flatpak-info").exists() {
        return Install::Flatpak;
    }
    let found = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|dir| dir.join("yt-dlp"))
        .find(|p| p.is_file());
    match found {
        Some(p) if p.starts_with("/usr") => Install::System,
        _ => Install::User,
    }
}

/// What to do to update, in a sentence.
pub fn update_hint(install: Install) -> String {
    let has = |tool: &str| Path::new("/usr/bin").join(tool).exists();
    match install {
        Install::Flatpak => "Update subfeed; it brings its own yt-dlp.".into(),
        Install::System if has("pacman") => "Update it with: sudo pacman -Syu yt-dlp".into(),
        Install::System if has("apt") => "Update it with: sudo apt update && sudo apt upgrade yt-dlp".into(),
        Install::System if has("dnf") => "Update it with: sudo dnf upgrade yt-dlp".into(),
        Install::System if has("zypper") => "Update it with: sudo zypper update yt-dlp".into(),
        Install::System => "Update it with your system's package manager.".into(),
        Install::User => "Update it with: yt-dlp -U (or pipx upgrade yt-dlp / pip install -U yt-dlp)".into(),
    }
}

/// The newest yt-dlp release on GitHub ("2026.10.02"). One small request; callers keep it rare.
pub fn latest_release() -> Result<String> {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
    }
    let mut resp = crate::thumbs::agent()
        .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("asking GitHub for yt-dlp's latest release")?;
    if resp.status() != 200 {
        bail!("GitHub answered HTTP {}", resp.status().as_u16());
    }
    let release: Release = serde_json::from_str(&resp.body_mut().read_to_string()?)?;
    Ok(release.tag_name.trim_start_matches('v').to_string())
}

/// "yt-dlp 2026.10.02 is out (you have 2026.08.19). Update it with: …", when that's so.
pub fn outdated_note() -> Option<String> {
    let current = version()?;
    let latest = latest_release().ok()?;
    is_newer(&latest, &current)
        .then(|| format!("yt-dlp {latest} is out (you have {current}). {}", update_hint(install())))
}

/// "yt-dlp 2026.08.19", or just "yt-dlp" if it can't say.
pub fn version_label() -> String {
    version().map_or_else(|| "yt-dlp".to_string(), |v| format!("yt-dlp {v}"))
}

/// Accepts a `UC…` id, a channel/handle/video URL (with or without `https://`), or a handle
/// with or without its `@`.
pub fn resolve_channel(input: &str) -> Result<Channel> {
    let input = input.trim();
    if is_channel_id(input) {
        return Ok(Channel { id: input.to_string(), title: String::new() });
    }
    let url = channel_lookup_url(input)?;
    let info: Info = serde_json::from_str(&run(&["--flat-playlist", "-I", "0", "-J", &url])?)
        .context("reading yt-dlp JSON")?;
    match info.channel_id {
        Some(id) if is_channel_id(&id) => Ok(Channel { id, title: info.channel.unwrap_or_default() }),
        _ => bail!("couldn't find a channel at {url}"),
    }
}

/// Where to ask yt-dlp about what someone typed for a channel.
fn channel_lookup_url(input: &str) -> Result<String> {
    let is_handle = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || "._-".contains(c));
    Ok(if let Some(handle) = input.strip_prefix('@') {
        format!("https://www.youtube.com/@{handle}")
    } else if input.starts_with("http://") || input.starts_with("https://") {
        input.to_string()
    } else if input.starts_with("youtube.com/") || input.starts_with("www.youtube.com/") || input.starts_with("m.youtube.com/") {
        format!("https://{input}")
    } else if is_handle(input) {
        format!("https://www.youtube.com/@{input}")
    } else {
        bail!("{input:?} is not a channel id, @handle or YouTube URL");
    })
}

pub fn fetch_tab(channel_id: &str, now: i64) -> Result<Tab> {
    let url = format!("{}/videos", channel_url(channel_id));
    let json = run(&[
        "--flat-playlist",
        "-I",
        TAB_ITEMS,
        "--extractor-args",
        "youtubetab:approximate_date",
        "-J",
        &url,
    ])?;
    parse_tab(&json, channel_id, now)
}

pub fn parse_tab(json: &str, channel_id: &str, now: i64) -> Result<Tab> {
    let info: Info = serde_json::from_str(json).context("reading yt-dlp JSON")?;
    let entries = info
        .entries
        .into_iter()
        // Upcoming premieres and running streams aren't watchable videos yet.
        .filter(|e| !matches!(e.live_status.as_deref(), Some("is_upcoming" | "is_live")))
        .map(|e| VideoEntry {
            thumb_url: Some(thumb_url(&e.id)),
            title: e.title.unwrap_or_default(),
            published_at: e.timestamp.unwrap_or(now),
            published_approx: true,
            duration_s: e.duration.map(|d| d.round() as i64),
            is_short: false,
            channel_id: channel_id.to_string(),
            id: e.id,
        })
        .collect();
    Ok(Tab { channel_title: info.channel, entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(version_date("2026.08.19"), chrono::NaiveDate::from_ymd_opt(2026, 8, 19));
        assert_eq!(version_date("2026.08.19.1"), chrono::NaiveDate::from_ymd_opt(2026, 8, 19));
        assert_eq!(version_date("nightly"), None);
        assert!(is_newer("2026.10.02", "2026.08.19"));
        assert!(is_newer("2026.08.19.1", "2026.08.19"));
        assert!(!is_newer("2026.08.19", "2026.08.19"));
        assert!(!is_newer("2025.12.30", "2026.01.01"));
        assert!(update_hint(Install::Flatpak).contains("Update subfeed"));
        assert!(update_hint(Install::User).contains("yt-dlp -U"));
    }

    #[test]
    fn channel_inputs_people_type() {
        let url = |s| channel_lookup_url(s).unwrap();
        assert_eq!(url("@Fireship"), "https://www.youtube.com/@Fireship");
        assert_eq!(url("fireship"), "https://www.youtube.com/@fireship");
        assert_eq!(url("youtube.com/@jeffnippard"), "https://youtube.com/@jeffnippard");
        assert_eq!(url("https://www.youtube.com/c/Old"), "https://www.youtube.com/c/Old");
        assert!(channel_lookup_url("two words").is_err());
    }

    #[test]
    fn parses_videos_tab() {
        let json = include_str!("../tests/fixtures/ytdlp_fireship_videos.json");
        let tab = parse_tab(json, "UCsBjURrPoezykLs9EqgamOA", 0).unwrap();
        assert_eq!(tab.channel_title.as_deref(), Some("Fireship"));
        assert_eq!(tab.entries.len(), 4, "the upcoming premiere is skipped");
        let first = &tab.entries[0];
        assert_eq!(first.id, "QZVzuq28eFw");
        assert_eq!(first.duration_s, Some(306));
        assert_eq!(first.published_at, 1_792_004_400);
        assert!(first.published_approx && !first.is_short);
        assert_eq!(first.thumb_url.as_deref(), Some("https://i.ytimg.com/vi/QZVzuq28eFw/hqdefault.jpg"));
    }

    #[test]
    fn channel_ids_resolve_without_yt_dlp() {
        let ch = resolve_channel(" UCsBjURrPoezykLs9EqgamOA ").unwrap();
        assert_eq!(ch.id, "UCsBjURrPoezykLs9EqgamOA");
        assert!(resolve_channel("not a channel").is_err());
    }
}
