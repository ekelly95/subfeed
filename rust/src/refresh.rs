//! `subfeed --refresh`: fetch every due channel and merge into SQLite.
//!
//! Per channel: RSS GET, conditional when the server gave an ETag / Last-Modified (YouTube's feed
//! server sends neither as of 2026-10, only `Cache-Control: max-age=900`, so in practice every
//! fetch is a full ~40 KB download and "unchanged" means "no new videos"). If RSS fails, fall back to the
//! yt-dlp `/videos` tab. Only when both fail does the channel back off (15 min doubling to 24 h,
//! with jitter). RSS 404s are usually a YouTube-wide outage, not the channel, so after
//! `OUTAGE_THRESHOLD` failures with no success the rest of the run skips RSS entirely.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Result, bail};

use crate::db::{ChannelRow, Db};
use crate::model::VideoEntry;
use crate::{rss, ytdlp};

const OUTAGE_THRESHOLD: usize = 5;
const BACKOFF_BASE_S: i64 = 15 * 60;
const BACKOFF_MAX_S: i64 = 24 * 60 * 60;

pub enum RssResponse {
    NotModified,
    Ok { body: String, etag: Option<String>, last_modified: Option<String> },
}

/// Where feeds come from. `HttpSource` in real use; fixtures in tests.
pub trait FeedSource: Sync {
    fn rss(&self, ch: &ChannelRow) -> Result<RssResponse>;
    fn tab(&self, channel_id: &str, now: i64) -> Result<ytdlp::Tab>;
}

pub struct HttpSource {
    agent: ureq::Agent,
}

impl HttpSource {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .user_agent(concat!("subfeed/", env!("CARGO_PKG_VERSION"), " (personal feed reader)"))
            .build()
            .into();
        Self { agent }
    }
}

impl Default for HttpSource {
    fn default() -> Self {
        Self::new()
    }
}

impl FeedSource for HttpSource {
    fn rss(&self, ch: &ChannelRow) -> Result<RssResponse> {
        let mut req = self.agent.get(rss::feed_url(&ch.id));
        if let Some(etag) = &ch.etag {
            req = req.header("If-None-Match", etag);
        }
        if let Some(lm) = &ch.last_modified {
            req = req.header("If-Modified-Since", lm);
        }
        let mut resp = req.call()?;
        let header = |name: &str| resp.headers().get(name).and_then(|v| v.to_str().ok()).map(String::from);
        match resp.status().as_u16() {
            304 => Ok(RssResponse::NotModified),
            200 => {
                let (etag, last_modified) = (header("etag"), header("last-modified"));
                let body = resp.body_mut().read_to_string()?;
                Ok(RssResponse::Ok { body, etag, last_modified })
            }
            code => bail!("RSS HTTP {code}"),
        }
    }

    fn tab(&self, channel_id: &str, now: i64) -> Result<ytdlp::Tab> {
        ytdlp::fetch_tab(channel_id, now)
    }
}

pub struct Options {
    /// Ignore backoff and fetch every channel.
    pub force: bool,
    /// Use yt-dlp when RSS fails.
    pub fallback: bool,
    pub workers: usize,
    /// Random pause before each request, to be polite. Off in tests.
    pub jitter: bool,
    /// At most this many channels get their missing video lengths looked up (one yt-dlp call
    /// each) per run; 0 turns it off.
    pub durations: usize,
    /// On battery: RSS only (a ~40 KB download per channel), no yt-dlp. A channel whose RSS
    /// fails is left for the next run on AC instead of backing off, so an RSS outage while
    /// unplugged doesn't delay it for hours once the fallback is available again.
    pub battery: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { force: false, fallback: true, workers: 4, jitter: true, durations: 10, battery: false }
    }
}

impl Options {
    /// The default options, made light when the machine is running on battery.
    pub fn for_power() -> Self {
        if on_battery() { Self::default().on_battery() } else { Self::default() }
    }

    /// RSS only: no yt-dlp fallback and no length lookups.
    pub fn on_battery(self) -> Self {
        Self { battery: true, fallback: false, durations: 0, ..self }
    }
}

/// On battery: a machine with a mains supply that's offline. Machines without one (desktops,
/// or no /sys access) count as on AC.
pub fn on_battery() -> bool {
    let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else { return false };
    let read = |p: &std::path::Path, f: &str| std::fs::read_to_string(p.join(f)).unwrap_or_default().trim().to_string();
    let mains: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| read(p, "type") == "Mains").collect();
    !mains.is_empty() && mains.iter().all(|p| read(p, "online") != "1")
}

#[derive(Debug, Default)]
pub struct Summary {
    pub due: usize,
    pub backing_off: usize,
    /// Fetched over RSS and brought new videos.
    pub updated: usize,
    /// Answered 304, or fetched over RSS with nothing new.
    pub unchanged: usize,
    pub via_fallback: usize,
    pub failed: usize,
    /// On battery: RSS failed and the channel waits for a run on AC (no backoff).
    pub deferred: usize,
    pub new_videos: usize,
    /// Video lengths filled in from yt-dlp (RSS has none).
    pub durations_filled: usize,
    /// New videos from channels that had been fetched before (a channel's first fetch brings
    /// in its whole backlog, which isn't news).
    pub fresh: Vec<String>,
    pub rss_outage: bool,
    pub errors: Vec<(String, String)>,
}

impl Summary {
    fn add_new(&mut self, ch: &ChannelRow, new: Vec<String>) {
        self.new_videos += new.len();
        if ch.last_fetch.is_some() {
            self.fresh.extend(new);
        }
    }
}

enum Outcome {
    Fetched { title: Option<String>, entries: Vec<VideoEntry>, etag: Option<String>, last_modified: Option<String> },
    NotModified,
    Fallback { title: Option<String>, entries: Vec<VideoEntry> },
    Failed(String),
    /// On battery, RSS failed: try again later, without backing off.
    Deferred,
}

struct Shared<'a> {
    src: &'a dyn FeedSource,
    opts: &'a Options,
    now: i64,
    rss_failures: AtomicUsize,
    rss_ok: AtomicBool,
}

impl Shared<'_> {
    fn rss_looks_down(&self) -> bool {
        !self.rss_ok.load(Ordering::Relaxed) && self.rss_failures.load(Ordering::Relaxed) >= OUTAGE_THRESHOLD
    }

    fn fetch(&self, ch: &ChannelRow) -> Outcome {
        let rss_err = if self.rss_looks_down() {
            "RSS is down".to_string()
        } else {
            self.pause();
            let result = self.src.rss(ch).and_then(|r| match r {
                RssResponse::NotModified => Ok(Outcome::NotModified),
                RssResponse::Ok { body, etag, last_modified } => {
                    let feed = rss::parse(&body)?;
                    Ok(Outcome::Fetched { title: feed.channel_title, entries: feed.entries, etag, last_modified })
                }
            });
            match result {
                Ok(outcome) => {
                    self.rss_ok.store(true, Ordering::Relaxed);
                    return outcome;
                }
                Err(e) => {
                    self.rss_failures.fetch_add(1, Ordering::Relaxed);
                    format!("{e:#}")
                }
            }
        };
        if self.opts.battery {
            return Outcome::Deferred;
        }
        if !self.opts.fallback {
            return Outcome::Failed(rss_err);
        }
        self.pause();
        match self.src.tab(&ch.id, self.now) {
            Ok(tab) => Outcome::Fallback { title: tab.channel_title, entries: tab.entries },
            Err(e) => Outcome::Failed(format!("{rss_err}; fallback: {e:#}")),
        }
    }

    fn pause(&self) {
        if self.opts.jitter {
            std::thread::sleep(Duration::from_millis(fastrand::u64(150..600)));
        }
    }
}

fn backoff(fail_count: i64) -> i64 {
    let base = (BACKOFF_BASE_S << fail_count.clamp(0, 10)).min(BACKOFF_MAX_S);
    base + fastrand::i64(0..=base / 5)
}

pub fn refresh(db: &mut Db, src: &dyn FeedSource, opts: &Options, now: i64) -> Result<Summary> {
    let mut summary = Summary::default();
    let mut due = VecDeque::new();
    for ch in db.channels()? {
        if !opts.force && ch.retry_after.is_some_and(|t| t > now) {
            summary.backing_off += 1;
        } else {
            due.push_back(ch);
        }
    }
    summary.due = due.len();
    if due.is_empty() {
        return Ok(summary);
    }

    let workers = opts.workers.clamp(1, due.len());
    let queue = Mutex::new(due);
    let shared = Shared { src, opts, now, rss_failures: AtomicUsize::new(0), rss_ok: AtomicBool::new(false) };
    let (tx, rx) = mpsc::channel::<(ChannelRow, Outcome)>();

    std::thread::scope(|s| -> Result<()> {
        for _ in 0..workers {
            let tx = tx.clone();
            let (queue, shared) = (&queue, &shared);
            s.spawn(move || {
                while let Some(ch) = queue.lock().unwrap().pop_front() {
                    let outcome = shared.fetch(&ch);
                    if tx.send((ch, outcome)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);

        // Only this thread touches the database.
        for (ch, outcome) in rx {
            match outcome {
                Outcome::Fetched { title, entries, etag, last_modified } => {
                    let new = db.upsert_videos(&entries, now)?;
                    if new.is_empty() {
                        summary.unchanged += 1;
                    } else {
                        summary.updated += 1;
                    }
                    summary.add_new(&ch, new);
                    db.record_success(&ch.id, now, title.as_deref(), etag.as_deref(), last_modified.as_deref())?;
                }
                Outcome::NotModified => {
                    db.record_success(&ch.id, now, None, None, None)?;
                    summary.unchanged += 1;
                }
                Outcome::Fallback { title, entries } => {
                    let new = db.upsert_videos(&entries, now)?;
                    summary.add_new(&ch, new);
                    db.record_success(&ch.id, now, title.as_deref(), None, None)?;
                    summary.via_fallback += 1;
                }
                Outcome::Failed(err) => {
                    db.record_failure(&ch.id, now + backoff(ch.fail_count), &err)?;
                    summary.failed += 1;
                    summary.errors.push((ch.title.clone(), err));
                }
                Outcome::Deferred => summary.deferred += 1,
            }
        }
        Ok(())
    })?;
    summary.rss_outage = shared.rss_looks_down();

    // RSS gives no lengths. One yt-dlp /videos call per channel fills them for its newest
    // uploads; a channel is asked again only once newer videos arrive. Skipped when yt-dlp is
    // off, and during an RSS outage (the fallback brought lengths already).
    if opts.fallback && opts.durations > 0 && !summary.rss_outage {
        for id in db.channels_missing_durations(opts.durations)? {
            shared.pause();
            // A failed lookup still counts as asked, so it isn't retried every run.
            let entries = src.tab(&id, now).map(|tab| tab.entries).unwrap_or_default();
            summary.durations_filled += db.fill_durations(&id, &entries, now)?;
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Channel;

    const FIRESHIP: &str = "UCsBjURrPoezykLs9EqgamOA";

    /// Serves the fixtures; RSS answers per `rss_status`.
    struct Fixtures {
        rss_status: u16,
        tab_ok: bool,
        etag: Option<&'static str>,
        rss_calls: AtomicUsize,
        tab_calls: AtomicUsize,
    }

    impl Fixtures {
        fn new(rss_status: u16, tab_ok: bool) -> Self {
            Self { rss_status, tab_ok, etag: Some("\"v1\""), rss_calls: AtomicUsize::new(0), tab_calls: AtomicUsize::new(0) }
        }

        /// Like YouTube today: no validators, so every fetch is a full 200.
        fn without_etag() -> Self {
            Self { etag: None, ..Self::new(200, true) }
        }
    }

    impl FeedSource for Fixtures {
        fn rss(&self, ch: &ChannelRow) -> Result<RssResponse> {
            self.rss_calls.fetch_add(1, Ordering::Relaxed);
            match self.rss_status {
                200 if self.etag.is_some() && ch.etag.as_deref() == self.etag => Ok(RssResponse::NotModified),
                200 => Ok(RssResponse::Ok {
                    body: include_str!("../tests/fixtures/rss_fireship.xml").into(),
                    etag: self.etag.map(Into::into),
                    last_modified: None,
                }),
                code => bail!("RSS HTTP {code}"),
            }
        }

        fn tab(&self, channel_id: &str, now: i64) -> Result<ytdlp::Tab> {
            self.tab_calls.fetch_add(1, Ordering::Relaxed);
            if !self.tab_ok {
                bail!("yt-dlp failed");
            }
            ytdlp::parse_tab(include_str!("../tests/fixtures/ytdlp_fireship_videos.json"), channel_id, now)
        }
    }

    fn opts() -> Options {
        Options { jitter: false, ..Options::default() }
    }

    fn db_with(ids: &[&str]) -> Db {
        let db = Db::open_in_memory().unwrap();
        for id in ids {
            db.add_channel(&Channel { id: id.to_string(), title: String::new() }, 0).unwrap();
        }
        db
    }

    #[test]
    fn rss_then_not_modified() {
        let mut db = db_with(&[FIRESHIP]);
        let src = Fixtures::new(200, true);

        let s = refresh(&mut db, &src, &opts(), 100).unwrap();
        assert_eq!((s.updated, s.new_videos, s.failed), (1, 3, 0));
        let ch = &db.channels().unwrap()[0];
        assert_eq!(ch.title, "Fireship", "title filled from the feed");
        assert_eq!(ch.etag.as_deref(), Some("\"v1\""));

        assert!(s.fresh.is_empty(), "a first fetch isn't news");

        let s = refresh(&mut db, &src, &opts(), 200).unwrap();
        assert_eq!((s.unchanged, s.new_videos), (1, 0));
        assert_eq!(db.channels().unwrap()[0].last_fetch, Some(200));
    }

    #[test]
    fn lengths_are_filled_once_per_batch_of_new_videos() {
        let mut db = db_with(&[FIRESHIP]);
        let src = Fixtures::without_etag();
        let s = refresh(&mut db, &src, &opts(), 100).unwrap();
        // RSS brought 3 videos without lengths; the /videos tab knows two of them.
        assert_eq!((s.new_videos, s.durations_filled), (3, 2));
        assert_eq!(src.tab_calls.load(Ordering::Relaxed), 1);
        let feed = db.feed(10, false).unwrap();
        assert_eq!(feed.iter().find(|v| v.id == "QZVzuq28eFw").unwrap().duration_s, Some(306));
        assert_eq!(feed.iter().find(|v| v.id == "No-JPdFvYWU").unwrap().duration_s, None, "older than the tab");
        assert_eq!(feed.len(), 3, "the tab's other videos aren't added");

        // Nothing newer: not asked again, though one length is still missing.
        refresh(&mut db, &src, &opts(), 200).unwrap();
        assert_eq!(src.tab_calls.load(Ordering::Relaxed), 1);

        // Off with --no-fallback.
        let mut db = db_with(&[FIRESHIP]);
        let src = Fixtures::without_etag();
        let s = refresh(&mut db, &src, &Options { fallback: false, ..opts() }, 100).unwrap();
        assert_eq!((s.durations_filled, src.tab_calls.load(Ordering::Relaxed)), (0, 0));
    }

    #[test]
    fn full_refetch_with_nothing_new_counts_as_unchanged() {
        let mut db = db_with(&[FIRESHIP]);
        let src = Fixtures::without_etag();
        let s = refresh(&mut db, &src, &opts(), 100).unwrap();
        assert_eq!((s.updated, s.unchanged, s.new_videos), (1, 0, 3));

        let s = refresh(&mut db, &src, &opts(), 200).unwrap();
        assert_eq!((s.updated, s.unchanged, s.new_videos), (0, 1, 0));
        assert_eq!(src.rss_calls.load(Ordering::Relaxed), 2, "no validators, so it fetched again");
        assert_eq!(db.channels().unwrap()[0].etag, None);
    }

    #[test]
    fn falls_back_to_yt_dlp_and_rss_later_fixes_dates() {
        let mut db = db_with(&[FIRESHIP]);
        let s = refresh(&mut db, &Fixtures::new(404, true), &opts(), 100).unwrap();
        assert_eq!((s.via_fallback, s.new_videos, s.failed), (1, 4, 0));
        assert!(db.feed(10, false).unwrap().iter().all(|v| v.published_approx));

        // RSS back: QZVzuq28eFw and wajoP_5Cpw8 get exact dates; No-JPdFvYWU (older than the
        // yt-dlp fixture's window) is new.
        let s = refresh(&mut db, &Fixtures::new(200, true), &opts(), 200).unwrap();
        assert_eq!((s.updated, s.new_videos), (1, 1));
        assert_eq!(s.fresh, ["No-JPdFvYWU"], "the second fetch's new video is news");
        let feed = db.feed(10, false).unwrap();
        let first = feed.iter().find(|v| v.id == "QZVzuq28eFw").unwrap();
        assert!(!first.published_approx);
        assert_eq!(first.published_at, 1_792_000_090); // 2026-10-14T17:48:10Z
        assert_eq!(first.duration_s, Some(306), "duration from yt-dlp is kept");
    }

    #[test]
    fn total_failure_backs_off() {
        let mut db = db_with(&[FIRESHIP]);
        let s = refresh(&mut db, &Fixtures::new(500, false), &opts(), 1000).unwrap();
        assert_eq!(s.failed, 1);
        let ch = &db.channels().unwrap()[0];
        assert_eq!(ch.fail_count, 1);
        let wait = ch.retry_after.unwrap() - 1000;
        assert!((BACKOFF_BASE_S..=BACKOFF_BASE_S * 6 / 5).contains(&wait), "waited {wait}");
        assert!(ch.last_error.as_deref().unwrap().contains("RSS HTTP 500"));

        // Still backing off: nothing fetched.
        let s = refresh(&mut db, &Fixtures::new(200, true), &opts(), 1001).unwrap();
        assert_eq!((s.due, s.backing_off), (0, 1));

        // --force ignores it, and success clears the backoff.
        let s = refresh(&mut db, &Fixtures::new(200, true), &Options { force: true, ..opts() }, 1002).unwrap();
        assert_eq!(s.updated, 1);
        let ch = &db.channels().unwrap()[0];
        assert_eq!((ch.fail_count, ch.retry_after, ch.last_error.as_deref()), (0, None, None));
    }

    #[test]
    fn rss_outage_stops_hammering_rss() {
        let ids: Vec<String> = (0..20).map(|i| format!("UC{i:022}")).collect();
        let mut db = db_with(&ids.iter().map(String::as_str).collect::<Vec<_>>());
        let src = Fixtures::new(404, true);
        let s = refresh(&mut db, &src, &Options { workers: 1, ..opts() }, 100).unwrap();
        assert!(s.rss_outage);
        assert_eq!(s.via_fallback, 20);
        assert_eq!(src.rss_calls.load(Ordering::Relaxed), OUTAGE_THRESHOLD);
    }

    #[test]
    fn on_battery_uses_rss_only() {
        let battery = || opts().on_battery();

        // RSS works: new videos as usual, but no yt-dlp length lookups.
        let mut db = db_with(&[FIRESHIP]);
        let src = Fixtures::without_etag();
        let s = refresh(&mut db, &src, &battery(), 100).unwrap();
        assert_eq!((s.updated, s.new_videos, s.durations_filled), (1, 3, 0));
        assert_eq!(src.tab_calls.load(Ordering::Relaxed), 0);

        // RSS fails: no fallback, no backoff; the channel waits for the next run.
        let mut db = db_with(&[FIRESHIP]);
        let src = Fixtures::new(404, true);
        let s = refresh(&mut db, &src, &battery(), 100).unwrap();
        assert_eq!((s.deferred, s.failed, s.via_fallback), (1, 0, 0));
        assert_eq!(src.tab_calls.load(Ordering::Relaxed), 0);
        let ch = &db.channels().unwrap()[0];
        assert_eq!((ch.fail_count, ch.retry_after, ch.last_fetch), (0, None, None));

        // Back on AC, the fallback runs at once.
        let s = refresh(&mut db, &src, &opts(), 101).unwrap();
        assert_eq!(s.via_fallback, 1);
    }

    #[test]
    fn backoff_is_capped() {
        assert!(backoff(30) <= BACKOFF_MAX_S * 6 / 5);
        assert!(backoff(2) >= BACKOFF_BASE_S * 4);
    }
}
