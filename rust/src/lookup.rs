//! Video lookups (`yt-dlp -J`), shared by the player and prefetching.
//!
//! Opening a video costs one yt-dlp call: ~3 s, or ~11 s in power-saver, where yt-dlp's
//! challenge solving is CPU-bound. The feed prefetches the video you settle on (hover or keyboard
//! focus), so opening it is usually instant. One prefetch runs at a time and only the newest
//! request waits behind it, so sweeping across the grid starts at most one extra lookup; a
//! budget keeps a long browse from looking like a bot to YouTube. Results are kept for 30
//! minutes (the stream URLs inside expire after a few hours).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::stream::{self, VideoInfo};

type Fetch = dyn Fn(&str) -> Result<VideoInfo> + Send + Sync;

pub struct Limits {
    /// How long a lookup stays usable.
    pub fresh: Duration,
    /// At most this many prefetches per `window`.
    pub budget: usize,
    pub window: Duration,
    pub max_cached: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { fresh: Duration::from_secs(30 * 60), budget: 20, window: Duration::from_secs(10 * 60), max_cached: 40 }
    }
}

pub struct Lookups {
    fetch: Box<Fetch>,
    limits: Limits,
    state: Mutex<State>,
    done: Condvar,
}

#[derive(Default)]
struct State {
    cache: HashMap<String, (Instant, VideoInfo)>,
    /// Being fetched right now (by the prefetch worker or by `get`).
    running: HashSet<String>,
    /// The newest prefetch request that hasn't started.
    wanted: Option<String>,
    worker: bool,
    /// When recent prefetches started, for the budget.
    started: VecDeque<Instant>,
}

impl Lookups {
    pub fn new(fetch: Box<Fetch>, limits: Limits) -> Arc<Self> {
        Arc::new(Self { fetch, limits, state: Mutex::default(), done: Condvar::new() })
    }

    /// The app-wide instance, backed by yt-dlp.
    pub fn global() -> &'static Arc<Lookups> {
        static GLOBAL: OnceLock<Arc<Lookups>> = OnceLock::new();
        GLOBAL.get_or_init(|| Lookups::new(Box::new(stream::fetch_info), Limits::default()))
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        s.cache.retain(|_, (at, _)| now.duration_since(*at) < self.limits.fresh);
        s
    }

    /// Looks `id` up in the background, unless it's cached or already being fetched. Cheap to
    /// call often; a newer request replaces one that hasn't started.
    pub fn prefetch(self: &Arc<Self>, id: &str) {
        let mut s = self.lock();
        if s.cache.contains_key(id) || s.running.contains(id) {
            return;
        }
        s.wanted = Some(id.to_string());
        if !s.worker {
            s.worker = true;
            let this = Arc::clone(self);
            std::thread::spawn(move || this.work());
        }
    }

    fn work(&self) {
        loop {
            let id = {
                let mut s = self.lock();
                let now = Instant::now();
                let window = self.limits.window;
                s.started.retain(|t| now.duration_since(*t) < window);
                let next = s.wanted.take().filter(|id| !s.cache.contains_key(id) && !s.running.contains(id));
                match next {
                    Some(id) if s.started.len() < self.limits.budget => {
                        s.started.push_back(now);
                        s.running.insert(id.clone());
                        id
                    }
                    _ => {
                        s.worker = false;
                        return;
                    }
                }
            };
            // A failed prefetch isn't kept: opening the video tries again and shows the error.
            let start = Instant::now();
            let result = (self.fetch)(&id);
            log(&format!("prefetched {id} in {:.1} s{}", start.elapsed().as_secs_f64(), if result.is_ok() { "" } else { " (failed)" }));
            self.finish(&id, result.ok());
        }
    }

    fn finish(&self, id: &str, info: Option<VideoInfo>) {
        let mut s = self.lock();
        s.running.remove(id);
        if let Some(info) = info {
            s.cache.insert(id.to_string(), (Instant::now(), info));
            while s.cache.len() > self.limits.max_cached {
                let Some(oldest) = s.cache.iter().min_by_key(|(_, (at, _))| *at).map(|(k, _)| k.clone()) else { break };
                s.cache.remove(&oldest);
            }
        }
        drop(s);
        self.done.notify_all();
    }

    /// The lookup for `id`: cached, or by waiting for one in flight, or fetched now.
    pub fn get(&self, id: &str) -> Result<VideoInfo> {
        let start = Instant::now();
        let mut s = self.lock();
        loop {
            if let Some((_, info)) = s.cache.get(id) {
                let waited = start.elapsed().as_secs_f64();
                log(&if waited < 0.05 { format!("open {id}: prefetched") } else { format!("open {id}: waited {waited:.1} s for its prefetch") });
                return Ok(info.clone());
            }
            if !s.running.contains(id) {
                break;
            }
            s = self.done.wait(s).unwrap_or_else(|e| e.into_inner());
        }
        // Queued but not started: fetch it here instead.
        if s.wanted.as_deref() == Some(id) {
            s.wanted = None;
        }
        s.running.insert(id.to_string());
        drop(s);
        let result = (self.fetch)(id);
        log(&format!("open {id}: looked up in {:.1} s (not prefetched)", start.elapsed().as_secs_f64()));
        self.finish(id, result.as_ref().ok().cloned());
        result
    }
}

/// One line on stderr (the journal, when started from the desktop), to see what prefetching
/// saves. Quiet in tests.
fn log(line: &str) {
    if !cfg!(test) {
        eprintln!("subfeed: {line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn info(id: &str) -> VideoInfo {
        let mut info =
            stream::parse_info(include_str!("../tests/fixtures/ytdlp_info_short_JVO00Ulr5BE.json")).unwrap();
        info.id = id.to_string();
        info
    }

    /// A fake yt-dlp that records calls and, when gated, waits for a go-ahead per call.
    struct Fake {
        calls: Arc<Mutex<Vec<String>>>,
        go: Option<Mutex<mpsc::Receiver<()>>>,
    }

    /// The lookups, the ids the fake was asked for, and (when gated) the go-ahead sender.
    type Rig = (Arc<Lookups>, Arc<Mutex<Vec<String>>>, Option<mpsc::Sender<()>>);

    fn lookups(gated: bool, limits: Limits) -> Rig {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel();
        let fake = Fake { calls: calls.clone(), go: gated.then(|| Mutex::new(rx)) };
        let fetch = move |id: &str| {
            fake.calls.lock().unwrap().push(id.to_string());
            if let Some(go) = &fake.go {
                go.lock().unwrap().recv().unwrap();
            }
            Ok(info(id))
        };
        (Lookups::new(Box::new(fetch), limits), calls, gated.then_some(tx))
    }

    fn wait_until(what: impl Fn() -> bool) {
        let start = Instant::now();
        while !what() {
            assert!(start.elapsed() < Duration::from_secs(5), "timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn open_after_prefetch_is_served_from_cache() {
        let (l, calls, _) = lookups(false, Limits::default());
        l.prefetch("a");
        wait_until(|| l.lock().cache.contains_key("a"));
        assert_eq!(l.get("a").unwrap().id, "a");
        assert_eq!(*calls.lock().unwrap(), ["a"]);
    }

    #[test]
    fn open_during_prefetch_waits_instead_of_fetching_again() {
        let (l, calls, go) = lookups(true, Limits::default());
        l.prefetch("a");
        wait_until(|| calls.lock().unwrap().len() == 1);
        let opener = {
            let l = Arc::clone(&l);
            std::thread::spawn(move || l.get("a").unwrap().id)
        };
        std::thread::sleep(Duration::from_millis(50));
        go.unwrap().send(()).unwrap();
        assert_eq!(opener.join().unwrap(), "a");
        assert_eq!(*calls.lock().unwrap(), ["a"]);
    }

    #[test]
    fn only_the_newest_request_waits_behind_a_running_one() {
        let (l, calls, go) = lookups(true, Limits::default());
        let go = go.unwrap();
        l.prefetch("a");
        wait_until(|| calls.lock().unwrap().len() == 1);
        l.prefetch("b");
        l.prefetch("c");
        go.send(()).unwrap();
        wait_until(|| calls.lock().unwrap().len() == 2);
        go.send(()).unwrap();
        wait_until(|| !l.lock().worker);
        assert_eq!(*calls.lock().unwrap(), ["a", "c"]);
    }

    #[test]
    fn budget_and_expiry() {
        let limits = Limits { budget: 1, fresh: Duration::ZERO, ..Limits::default() };
        let (l, calls, _) = lookups(false, limits);
        l.prefetch("a");
        wait_until(|| !l.lock().worker);
        l.prefetch("b");
        wait_until(|| !l.lock().worker);
        assert_eq!(*calls.lock().unwrap(), ["a"], "over budget: b isn't prefetched");

        // Opening always looks up, and with `fresh` at zero nothing is reused.
        l.get("a").unwrap();
        l.get("a").unwrap();
        assert_eq!(calls.lock().unwrap().len(), 3);
    }
}
