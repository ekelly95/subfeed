//! Thumbnail cache at `~/.cache/subfeed/thumbs/<video id>.jpg` (override with `SUBFEED_CACHE`).
//! Filled by `--refresh` so the app opens with images ready; the UI fetches whatever is missing.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Result, bail};

pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("SUBFEED_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::cache_dir().unwrap_or_else(|| PathBuf::from(".")).join("subfeed"));
    base.join("thumbs")
}

/// Video ids come from the network; only these characters may become a file name.
pub fn is_video_id(id: &str) -> bool {
    (6..=20).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn path_for(dir: &Path, video_id: &str) -> Option<PathBuf> {
    is_video_id(video_id).then(|| dir.join(format!("{video_id}.jpg")))
}

/// The 1280×720 frame used for the feed's hero; not every video has one.
pub fn hero_path_for(dir: &Path, video_id: &str) -> Option<PathBuf> {
    is_video_id(video_id).then(|| dir.join(format!("{video_id}.max.jpg")))
}

pub fn hero_source_url(video_id: &str) -> String {
    format!("https://i.ytimg.com/vi/{video_id}/maxresdefault.jpg")
}

pub fn cached_hero_url(dir: &Path, video_id: &str) -> Option<String> {
    hero_path_for(dir, video_id).filter(|p| p.is_file()).map(|p| format!("file://{}", p.display()))
}

/// `file://` URL of the cached thumbnail, if there is one.
pub fn cached_url(dir: &Path, video_id: &str) -> Option<String> {
    path_for(dir, video_id).filter(|p| p.is_file()).map(|p| format!("file://{}", p.display()))
}

pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .user_agent(concat!("subfeed/", env!("CARGO_PKG_VERSION"), " (personal feed reader)"))
        .build()
        .into()
}

fn looks_like_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF])          // JPEG
        || bytes.starts_with(b"\x89PNG")            // PNG
        || (bytes.len() > 12 && &bytes[8..12] == b"WEBP")
}

pub fn download(agent: &ureq::Agent, url: &str, dest: &Path) -> Result<()> {
    if !url.starts_with("https://") || !url.contains(".ytimg.com/") {
        bail!("not a YouTube thumbnail URL: {url}");
    }
    let mut resp = agent.get(url).call()?;
    if resp.status() != 200 {
        bail!("HTTP {}", resp.status());
    }
    let bytes = resp.body_mut().with_config().limit(2 * 1024 * 1024).read_to_vec()?;
    if !looks_like_image(&bytes) {
        bail!("not an image");
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Write then rename, so a reader never sees half a file.
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

/// Downloads every `(video id, url)` not yet cached, `workers` at a time. `on_done` runs on a
/// worker thread for each successful download. Returns (downloaded, failed).
pub fn fetch_missing(
    dir: &Path,
    items: Vec<(String, String)>,
    workers: usize,
    on_done: &(dyn Fn(&str, &Path) + Sync),
) -> (usize, usize) {
    let todo: VecDeque<_> = items
        .into_iter()
        .filter_map(|(id, url)| path_for(dir, &id).filter(|p| !p.exists()).map(|p| (id, url, p)))
        .collect();
    if todo.is_empty() {
        return (0, 0);
    }
    let workers = workers.clamp(1, todo.len());
    let queue = Mutex::new(todo);
    let counts = Mutex::new((0, 0));
    let agent = agent();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                while let Some((id, url, path)) = queue.lock().unwrap().pop_front() {
                    let ok = download(&agent, &url, &path).is_ok();
                    if ok {
                        on_done(&id, &path);
                    }
                    let mut c = counts.lock().unwrap();
                    if ok { c.0 += 1 } else { c.1 += 1 }
                }
            });
        }
    });
    counts.into_inner().unwrap()
}

/// Deletes cached thumbnails for videos no longer in the database. Returns how many.
pub fn prune(dir: &Path, keep: &HashSet<String>) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            let stem = name.strip_suffix(".jpg").or_else(|| name.strip_suffix(".part"));
            let stem = stem.map(|s| s.strip_suffix(".max").unwrap_or(s));
            stem.is_some_and(|id| !keep.contains(id))
        })
        .filter(|e| std::fs::remove_file(e.path()).is_ok())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_safe_file_names() {
        let dir = Path::new("/c");
        assert_eq!(path_for(dir, "QZVzuq28eFw"), Some(PathBuf::from("/c/QZVzuq28eFw.jpg")));
        assert_eq!(path_for(dir, "../../etc/x"), None);
        assert_eq!(path_for(dir, "a/b"), None);
        assert_eq!(path_for(dir, ""), None);
    }

    #[test]
    fn image_sniffing() {
        assert!(looks_like_image(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(looks_like_image(b"RIFF\0\0\0\0WEBPVP8 "));
        assert!(!looks_like_image(b"<!DOCTYPE html>"));
    }

    #[test]
    fn prune_keeps_known_videos() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["keep1234567.jpg", "keep1234567.max.jpg", "gone1234567.jpg", "gone1234567.max.jpg", "half1234567.part", "notes.txt"] {
            std::fs::write(dir.path().join(f), b"x").unwrap();
        }
        let keep = HashSet::from(["keep1234567".to_string()]);
        assert_eq!(prune(dir.path(), &keep), 3);
        assert!(dir.path().join("keep1234567.max.jpg").exists());
        assert!(dir.path().join("keep1234567.jpg").exists());
        assert!(dir.path().join("notes.txt").exists());
        assert_eq!(cached_url(dir.path(), "keep1234567").unwrap(), format!("file://{}/keep1234567.jpg", dir.path().display()));
        assert_eq!(cached_url(dir.path(), "gone1234567"), None);
    }
}
