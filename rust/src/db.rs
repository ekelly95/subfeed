//! SQLite storage at `~/.local/share/subfeed/subfeed.db` (override with `SUBFEED_DB`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::model::{Channel, VideoEntry};

/// Each entry upgrades the schema by one `user_version`. Never edit a shipped entry; append.
const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE channels (
    id            TEXT PRIMARY KEY,          -- UC…
    title         TEXT NOT NULL,
    added_at      INTEGER NOT NULL,
    folder        TEXT,
    last_fetch    INTEGER,                   -- last successful fetch
    etag          TEXT,
    last_modified TEXT,
    fail_count    INTEGER NOT NULL DEFAULT 0,
    retry_after   INTEGER,                   -- backoff: skip until this time
    last_error    TEXT
);
CREATE TABLE videos (
    id               TEXT PRIMARY KEY,
    channel_id       TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    title            TEXT NOT NULL,
    published_at     INTEGER NOT NULL,
    published_approx INTEGER NOT NULL DEFAULT 0,
    duration_s       INTEGER,
    thumb_url        TEXT,
    is_short         INTEGER NOT NULL DEFAULT 0,
    first_seen       INTEGER NOT NULL,
    watched_at       INTEGER,
    hidden           INTEGER NOT NULL DEFAULT 0,
    position_s       REAL NOT NULL DEFAULT 0
);
CREATE INDEX videos_by_date ON videos(published_at DESC);
CREATE INDEX videos_by_channel ON videos(channel_id, published_at DESC);
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT
);
"#, r#"
-- Filled when a video is opened (one yt-dlp call), so the player panel and notes have them.
ALTER TABLE videos ADD COLUMN description TEXT;
ALTER TABLE videos ADD COLUMN chapters_json TEXT;
ALTER TABLE videos ADD COLUMN last_played_at INTEGER;
CREATE INDEX videos_by_last_played ON videos(last_played_at DESC) WHERE last_played_at IS NOT NULL;
"#, r#"
-- RSS has no video lengths; refresh fills them from yt-dlp's /videos tab. When that last ran for
-- a channel, so it only runs again once newer videos arrive.
ALTER TABLE channels ADD COLUMN durations_at INTEGER;
"#];

pub fn default_path() -> PathBuf {
    if let Some(p) = std::env::var_os("SUBFEED_DB") {
        return PathBuf::from(p);
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("subfeed")
        .join("subfeed.db")
}

#[derive(Debug, Clone)]
pub struct ChannelRow {
    pub id: String,
    pub title: String,
    pub last_fetch: Option<i64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub fail_count: i64,
    pub retry_after: Option<i64>,
    pub last_error: Option<String>,
    pub video_count: i64,
}

#[derive(Debug, Clone)]
pub struct FeedRow {
    pub id: String,
    pub channel_title: String,
    pub title: String,
    pub published_at: i64,
    pub published_approx: bool,
    pub duration_s: Option<i64>,
    pub watched: bool,
    pub channel_id: String,
    pub thumb_url: Option<String>,
    pub position_s: f64,
    pub is_short: bool,
}

/// What the feed shows. The default is every channel, Shorts excluded.
#[derive(Debug, Clone)]
pub struct FeedFilter {
    pub channel_id: Option<String>,
    /// Case-insensitive match on the video or channel title.
    pub search: Option<String>,
    pub unwatched_only: bool,
    pub include_shorts: bool,
    /// Only Shorts (the Shorts view); overrides `include_shorts`.
    pub shorts_only: bool,
    pub limit: usize,
}

impl Default for FeedFilter {
    fn default() -> Self {
        Self { channel_id: None, search: None, unwatched_only: false, include_shorts: false, shorts_only: false, limit: 500 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifyRow {
    pub id: String,
    pub title: String,
    pub channel_title: String,
    pub published_at: i64,
    pub is_short: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelSummary {
    pub id: String,
    pub title: String,
    pub unwatched: i64,
    pub unwatched_shorts: i64,
    /// Visible videos and Shorts.
    pub video_count: i64,
    /// Newest upload, if any.
    pub latest_published: Option<i64>,
    /// Set while refreshing this channel keeps failing (both RSS and yt-dlp).
    pub last_error: Option<String>,
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        // WAL lets the UI read while a background `--refresh` writes.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        // SQLite's lower() only knows ASCII, so "ÉMILE" wouldn't match "émile". fold() is Rust's
        // full Unicode lowercase, for search. (Accents still count: "cafe" doesn't find "café".)
        conn.create_scalar_function(
            "fold",
            1,
            rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| Ok(ctx.get::<Option<String>>(0)?.map(|s| s.to_lowercase())),
        )?;
        let mut db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&mut self) -> Result<()> {
        let version: i64 = self.conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql).with_context(|| format!("migration {}", i + 1))?;
            tx.pragma_update(None, "user_version", i as i64 + 1)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Adds a channel; returns false if it was already there. Never removes or renames on re-import,
    /// except to fill in a missing title.
    pub fn add_channel(&self, ch: &Channel, now: i64) -> Result<bool> {
        let added = self.conn.execute(
            "INSERT OR IGNORE INTO channels(id, title, added_at) VALUES (?1, ?2, ?3)",
            params![ch.id, ch.title, now],
        )? == 1;
        if !added && !ch.title.is_empty() {
            self.conn.execute(
                "UPDATE channels SET title = ?2 WHERE id = ?1 AND title = ''",
                params![ch.id, ch.title],
            )?;
        }
        Ok(added)
    }

    pub fn remove_channel(&self, id: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM channels WHERE id = ?1", [id])? == 1)
    }

    pub fn channels(&self) -> Result<Vec<ChannelRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.title, c.last_fetch, c.etag, c.last_modified, c.fail_count,
                    c.retry_after, c.last_error,
                    (SELECT COUNT(*) FROM videos v WHERE v.channel_id = c.id)
             FROM channels c ORDER BY c.title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ChannelRow {
                id: r.get(0)?,
                title: r.get(1)?,
                last_fetch: r.get(2)?,
                etag: r.get(3)?,
                last_modified: r.get(4)?,
                fail_count: r.get(5)?,
                retry_after: r.get(6)?,
                last_error: r.get(7)?,
                video_count: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn record_success(
        &self,
        id: &str,
        now: i64,
        title: Option<&str>,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE channels SET last_fetch = ?2, fail_count = 0, retry_after = NULL, last_error = NULL,
                 title = COALESCE(NULLIF(?3, ''), title),
                 etag = COALESCE(?4, etag), last_modified = COALESCE(?5, last_modified)
             WHERE id = ?1",
            params![id, now, title, etag, last_modified],
        )?;
        Ok(())
    }

    /// Counts a failed fetch and backs the channel off until `retry_after`.
    pub fn record_failure(&self, id: &str, retry_after: i64, error: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE channels SET fail_count = fail_count + 1, retry_after = ?2, last_error = ?3 WHERE id = ?1",
            params![id, retry_after, error],
        )?;
        Ok(())
    }

    /// Merges entries into `videos` and returns the ids of the ones that were new.
    ///
    /// An exact date always replaces an approximate one; an approximate date never replaces anything.
    /// Watched/hidden/position are the user's and are never touched here, and an empty title
    /// (the yt-dlp fallback can give one) never replaces a real one.
    pub fn upsert_videos(&mut self, entries: &[VideoEntry], now: i64) -> Result<Vec<String>> {
        let tx = self.conn.transaction()?;
        let mut new = Vec::new();
        {
            let mut exists = tx.prepare_cached("SELECT 1 FROM videos WHERE id = ?1")?;
            let mut upsert = tx.prepare_cached(
                "INSERT INTO videos(id, channel_id, title, published_at, published_approx,
                                    duration_s, thumb_url, is_short, first_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                    title = COALESCE(NULLIF(excluded.title, ''), title),
                    published_at = CASE WHEN excluded.published_approx = 0
                                        THEN excluded.published_at ELSE published_at END,
                    published_approx = published_approx AND excluded.published_approx,
                    duration_s = COALESCE(excluded.duration_s, duration_s),
                    thumb_url = COALESCE(excluded.thumb_url, thumb_url),
                    is_short = MAX(is_short, excluded.is_short)",
            )?;
            for e in entries {
                if exists.query_row([&e.id], |_| Ok(())).optional()?.is_none() {
                    new.push(e.id.clone());
                }
                upsert.execute(params![
                    e.id,
                    e.channel_id,
                    e.title,
                    e.published_at,
                    e.published_approx,
                    e.duration_s,
                    e.thumb_url,
                    e.is_short,
                    now
                ])?;
            }
        }
        tx.commit()?;
        Ok(new)
    }

    /// Newest first, without hidden videos (and without Shorts unless asked).
    pub fn feed(&self, limit: usize, include_shorts: bool) -> Result<Vec<FeedRow>> {
        self.query_feed(&FeedFilter { limit, include_shorts, ..FeedFilter::default() })
    }

    pub fn query_feed(&self, f: &FeedFilter) -> Result<Vec<FeedRow>> {
        let search = f.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let mut stmt = self.conn.prepare_cached(
            "SELECT v.id, c.title, v.title, v.published_at, v.published_approx, v.duration_s,
                    v.watched_at IS NOT NULL, v.channel_id, v.thumb_url, v.position_s, v.is_short
             FROM videos v JOIN channels c ON c.id = v.channel_id
             WHERE v.hidden = 0
               AND (CASE WHEN ?6 THEN v.is_short = 1 ELSE (?1 OR v.is_short = 0) END)
               AND (?2 IS NULL OR v.channel_id = ?2)
               AND (NOT ?3 OR v.watched_at IS NULL)
               AND (?4 IS NULL OR instr(fold(v.title), fold(?4)) OR instr(fold(c.title), fold(?4)))
             ORDER BY v.published_at DESC, v.id LIMIT ?5",
        )?;
        let rows = stmt.query_map(
            params![f.include_shorts, f.channel_id, f.unwatched_only, search, f.limit as i64, f.shorts_only],
            |r| {
                Ok(FeedRow {
                    id: r.get(0)?,
                    channel_title: r.get(1)?,
                    title: r.get(2)?,
                    published_at: r.get(3)?,
                    published_approx: r.get(4)?,
                    duration_s: r.get(5)?,
                    watched: r.get(6)?,
                    channel_id: r.get(7)?,
                    thumb_url: r.get(8)?,
                    position_s: r.get(9)?,
                    is_short: r.get(10)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Channels for the pills and the Channels page: unwatched videos (and, separately, Shorts),
    /// how many there are, the newest upload, and any refresh error.
    pub fn channel_summaries(&self) -> Result<Vec<ChannelSummary>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT c.id, c.title,
                    (SELECT COUNT(*) FROM videos v WHERE v.channel_id = c.id
                       AND v.hidden = 0 AND v.is_short = 0 AND v.watched_at IS NULL),
                    (SELECT COUNT(*) FROM videos v WHERE v.channel_id = c.id
                       AND v.hidden = 0 AND v.is_short = 1 AND v.watched_at IS NULL),
                    (SELECT COUNT(*) FROM videos v WHERE v.channel_id = c.id AND v.hidden = 0),
                    (SELECT MAX(v.published_at) FROM videos v WHERE v.channel_id = c.id),
                    CASE WHEN c.fail_count > 0 THEN c.last_error END
             FROM channels c ORDER BY c.title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ChannelSummary {
                id: r.get(0)?,
                title: r.get(1)?,
                unwatched: r.get(2)?,
                unwatched_shorts: r.get(3)?,
                video_count: r.get(4)?,
                latest_published: r.get(5)?,
                last_error: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Channels with videos that still have no length and arrived since their last length lookup,
    /// most recently active first.
    pub fn channels_missing_durations(&self, limit: usize) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT c.id FROM channels c
             WHERE EXISTS (SELECT 1 FROM videos v WHERE v.channel_id = c.id
                             AND v.duration_s IS NULL AND v.is_short = 0 AND v.hidden = 0
                             AND v.first_seen > COALESCE(c.durations_at, -1))
             ORDER BY (SELECT MAX(v.published_at) FROM videos v WHERE v.channel_id = c.id) DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Fills in lengths for videos already stored (never adds any) and notes that this channel
    /// was looked up. Returns how many lengths were filled.
    pub fn fill_durations(&mut self, channel_id: &str, entries: &[VideoEntry], now: i64) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut filled = 0;
        {
            let mut update =
                tx.prepare_cached("UPDATE videos SET duration_s = ?2 WHERE id = ?1 AND duration_s IS NULL")?;
            for e in entries {
                if let Some(d) = e.duration_s {
                    filled += update.execute(params![e.id, d])?;
                }
            }
        }
        tx.execute("UPDATE channels SET durations_at = ?2 WHERE id = ?1", params![channel_id, now])?;
        tx.commit()?;
        Ok(filled)
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn set_watched(&self, video_id: &str, watched: bool, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE videos SET watched_at = CASE WHEN ?2 THEN COALESCE(watched_at, ?3) END WHERE id = ?1",
            params![video_id, watched, now],
        )?;
        Ok(())
    }

    pub fn set_hidden(&self, video_id: &str, hidden: bool) -> Result<()> {
        self.conn.execute("UPDATE videos SET hidden = ?2 WHERE id = ?1", params![video_id, hidden])?;
        Ok(())
    }

    /// Stores what opening a video taught us. An exact upload time replaces an approximate one.
    pub fn set_details(
        &self,
        video_id: &str,
        description: Option<&str>,
        chapters_json: &str,
        duration_s: Option<i64>,
        published_at: Option<i64>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE videos SET description = ?2, chapters_json = ?3,
                    duration_s = COALESCE(?4, duration_s),
                    published_at = COALESCE(?5, published_at),
                    published_approx = published_approx AND ?5 IS NULL
             WHERE id = ?1",
            params![video_id, description, chapters_json, duration_s, published_at],
        )?;
        Ok(())
    }

    /// Saves where playback is. Called every few seconds while playing and when leaving.
    pub fn set_position(&self, video_id: &str, position_s: f64, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE videos SET position_s = ?2, last_played_at = ?3 WHERE id = ?1",
            params![video_id, position_s.max(0.0), now],
        )?;
        Ok(())
    }

    pub fn position(&self, video_id: &str) -> Result<f64> {
        Ok(self
            .conn
            .query_row("SELECT position_s FROM videos WHERE id = ?1", [video_id], |r| r.get(0))
            .optional()?
            .unwrap_or(0.0))
    }

    /// Started but not finished, most recently played first.
    pub fn continue_watching(&self, limit: usize) -> Result<Vec<FeedRow>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT v.id, c.title, v.title, v.published_at, v.published_approx, v.duration_s,
                    v.watched_at IS NOT NULL, v.channel_id, v.thumb_url, v.position_s, v.is_short
             FROM videos v JOIN channels c ON c.id = v.channel_id
             WHERE v.last_played_at IS NOT NULL AND v.watched_at IS NULL AND v.hidden = 0
               AND v.is_short = 0 AND v.position_s >= 30
             ORDER BY v.last_played_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| {
            Ok(FeedRow {
                id: r.get(0)?,
                channel_title: r.get(1)?,
                title: r.get(2)?,
                published_at: r.get(3)?,
                published_approx: r.get(4)?,
                duration_s: r.get(5)?,
                watched: r.get(6)?,
                channel_id: r.get(7)?,
                thumb_url: r.get(8)?,
                position_s: r.get(9)?,
                is_short: r.get(10)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Videos by id with what a notification needs, newest first. Hidden ones are left out.
    pub fn notify_candidates(&self, ids: &[String]) -> Result<Vec<NotifyRow>> {
        let mut out = Vec::new();
        let mut stmt = self.conn.prepare_cached(
            "SELECT v.id, v.title, c.title, v.published_at, v.is_short
             FROM videos v JOIN channels c ON c.id = v.channel_id WHERE v.id = ?1 AND v.hidden = 0",
        )?;
        for id in ids {
            if let Some(row) = stmt
                .query_row([id], |r| {
                    Ok(NotifyRow {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        channel_title: r.get(2)?,
                        published_at: r.get(3)?,
                        is_short: r.get(4)?,
                    })
                })
                .optional()?
            {
                out.push(row);
            }
        }
        out.sort_by_key(|r| std::cmp::Reverse(r.published_at));
        Ok(out)
    }

    /// When any channel was last fetched successfully.
    pub fn last_fetch(&self) -> Result<Option<i64>> {
        Ok(self.conn.query_row("SELECT MAX(last_fetch) FROM channels", [], |r| r.get(0))?)
    }

    /// Every video's id and thumbnail URL, for the thumbnail cache.
    pub fn thumb_targets(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT id, thumb_url FROM videos WHERE thumb_url IS NOT NULL")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(id: &str, title: &str) -> Channel {
        Channel { id: id.into(), title: title.into() }
    }

    fn video(id: &str, published_at: i64, approx: bool) -> VideoEntry {
        VideoEntry {
            id: id.into(),
            channel_id: "UCsBjURrPoezykLs9EqgamOA".into(),
            title: format!("video {id}"),
            published_at,
            published_approx: approx,
            duration_s: None,
            thumb_url: None,
            is_short: false,
        }
    }

    #[test]
    fn search_ignores_case_beyond_ascii() {
        let mut db = Db::open_in_memory().unwrap();
        db.add_channel(&ch("UCsBjURrPoezykLs9EqgamOA", "Fireship"), 0).unwrap();
        let mut accented = video("a", 50, false);
        accented.title = "ÉMILE ZOLA, ÜBER ALLES".into();
        db.upsert_videos(&[accented, video("b", 60, false)], 0).unwrap();
        let ids = |q: &str| {
            db.query_feed(&FeedFilter { search: Some(q.into()), ..Default::default() })
                .unwrap()
                .into_iter()
                .map(|r| r.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("émile"), ["a"]);
        assert_eq!(ids("über"), ["a"]);
        assert!(ids("emile").is_empty(), "accents still count");
    }

    #[test]
    fn settings_round_trip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.setting("k").unwrap(), None);
        db.set_setting("k", "1").unwrap();
        db.set_setting("k", "2").unwrap();
        assert_eq!(db.setting("k").unwrap().as_deref(), Some("2"));
    }

    #[test]
    fn empty_title_keeps_the_real_one() {
        let mut db = Db::open_in_memory().unwrap();
        db.add_channel(&ch("UCsBjURrPoezykLs9EqgamOA", "Fireship"), 0).unwrap();
        db.upsert_videos(&[video("a", 100, false)], 0).unwrap();
        let mut blank = video("a", 100, true);
        blank.title = String::new();
        db.upsert_videos(&[blank], 1).unwrap();
        assert_eq!(db.feed(10, false).unwrap()[0].title, "video a");
    }

    #[test]
    fn migrations_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/subfeed.db");
        Db::open(&path).unwrap();
        let db = Db::open(&path).unwrap();
        let v: i64 = db.conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
    }

    #[test]
    fn add_channel_never_duplicates_and_fills_missing_title() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.add_channel(&ch("UCsBjURrPoezykLs9EqgamOA", ""), 1).unwrap());
        assert!(!db.add_channel(&ch("UCsBjURrPoezykLs9EqgamOA", "Fireship"), 2).unwrap());
        assert!(!db.add_channel(&ch("UCsBjURrPoezykLs9EqgamOA", "Renamed"), 3).unwrap());
        let rows = db.channels().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Fireship");
    }

    #[test]
    fn exact_dates_win_and_user_state_survives() {
        let mut db = Db::open_in_memory().unwrap();
        db.add_channel(&ch("UCsBjURrPoezykLs9EqgamOA", "Fireship"), 0).unwrap();

        // Fallback source first: approximate date.
        assert_eq!(db.upsert_videos(&[video("a", 1000, true)], 10).unwrap().len(), 1);
        db.conn.execute("UPDATE videos SET watched_at = 5, position_s = 42 WHERE id = 'a'", []).unwrap();

        // RSS later: exact date replaces it, nothing counted as new.
        assert_eq!(db.upsert_videos(&[video("a", 1234, false)], 20).unwrap().len(), 0);
        // A later approximate date must not undo it.
        assert_eq!(db.upsert_videos(&[video("a", 999, true)], 30).unwrap().len(), 0);

        let (published, approx, watched, pos, first_seen): (i64, bool, Option<i64>, f64, i64) = db
            .conn
            .query_row(
                "SELECT published_at, published_approx, watched_at, position_s, first_seen FROM videos WHERE id='a'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!((published, approx, watched, pos, first_seen), (1234, false, Some(5), 42.0, 10));
    }

    #[test]
    fn feed_is_newest_first_and_hides_shorts() {
        let mut db = Db::open_in_memory().unwrap();
        db.add_channel(&ch("UCsBjURrPoezykLs9EqgamOA", "Fireship"), 0).unwrap();
        let mut short = video("s", 3000, false);
        short.is_short = true;
        db.upsert_videos(&[video("old", 1000, false), video("new", 2000, false), short], 0).unwrap();

        let ids: Vec<_> = db.feed(10, false).unwrap().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, ["new", "old"]);
        assert_eq!(db.feed(10, true).unwrap()[0].id, "s");

        let shorts = db.query_feed(&FeedFilter { shorts_only: true, ..FeedFilter::default() }).unwrap();
        assert_eq!(shorts.iter().map(|r| (r.id.as_str(), r.is_short)).collect::<Vec<_>>(), [("s", true)]);
        let summary = &db.channel_summaries().unwrap()[0];
        assert_eq!((summary.unwatched, summary.unwatched_shorts), (2, 1));
        assert_eq!((summary.video_count, summary.latest_published, summary.last_error.as_deref()), (3, Some(3000), None));

        // Removing a channel takes its videos with it.
        assert!(db.remove_channel("UCsBjURrPoezykLs9EqgamOA").unwrap());
        assert!(db.feed(10, true).unwrap().is_empty());
    }

    #[test]
    fn filters_watched_hidden_and_counts() {
        let mut db = Db::open_in_memory().unwrap();
        db.add_channel(&ch("UCsBjURrPoezykLs9EqgamOA", "Fireship"), 0).unwrap();
        db.add_channel(&ch("UCBJycsmduvYEL83R_U4JriQ", "Marques Brownlee"), 0).unwrap();
        let mut mkbhd = video("m", 500, false);
        mkbhd.channel_id = "UCBJycsmduvYEL83R_U4JriQ".into();
        mkbhd.title = "Pixel REVIEW".into();
        db.upsert_videos(&[video("a", 100, false), video("b", 200, false), mkbhd], 0).unwrap();

        let ids = |f: FeedFilter| db.query_feed(&f).unwrap().into_iter().map(|r| r.id).collect::<Vec<_>>();
        let fireship = Some("UCsBjURrPoezykLs9EqgamOA".to_string());

        assert_eq!(ids(FeedFilter { channel_id: fireship.clone(), ..Default::default() }), ["b", "a"]);
        assert_eq!(ids(FeedFilter { search: Some("review".into()), ..Default::default() }), ["m"]);
        assert_eq!(ids(FeedFilter { search: Some("marques".into()), ..Default::default() }), ["m"], "matches channel");
        assert_eq!(ids(FeedFilter { search: Some("  ".into()), ..Default::default() }).len(), 3);

        db.set_watched("b", true, 7).unwrap();
        db.set_watched("b", true, 9).unwrap();
        let watched_at: i64 = db.conn.query_row("SELECT watched_at FROM videos WHERE id='b'", [], |r| r.get(0)).unwrap();
        assert_eq!(watched_at, 7, "first watch time is kept");
        assert_eq!(ids(FeedFilter { unwatched_only: true, ..Default::default() }), ["m", "a"]);

        db.set_hidden("m", true).unwrap();
        assert_eq!(ids(FeedFilter::default()), ["b", "a"]);
        let counts: Vec<_> = db.channel_summaries().unwrap().into_iter().map(|c| (c.title, c.unwatched)).collect();
        assert_eq!(counts, [("Fireship".to_string(), 1), ("Marques Brownlee".to_string(), 0)]);

        db.set_hidden("m", false).unwrap();
        db.set_watched("b", false, 0).unwrap();
        assert_eq!(ids(FeedFilter { unwatched_only: true, ..Default::default() }), ["m", "b", "a"]);
    }

    #[test]
    fn positions_details_and_continue_watching() {
        let mut db = Db::open_in_memory().unwrap();
        db.add_channel(&ch("UCsBjURrPoezykLs9EqgamOA", "Fireship"), 0).unwrap();
        db.upsert_videos(&[video("a", 100, true), video("b", 200, false), video("c", 300, false)], 0).unwrap();

        db.set_position("a", 120.0, 50).unwrap();
        db.set_position("b", 10.0, 60).unwrap(); // too early to count as started
        db.set_position("c", 400.0, 70).unwrap();
        db.set_watched("c", true, 71).unwrap();
        let ids: Vec<_> = db.continue_watching(5).unwrap().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, ["a"]);
        assert_eq!(db.position("a").unwrap(), 120.0);
        assert_eq!(db.position("missing").unwrap(), 0.0);

        db.set_details("a", Some("desc"), "[]", Some(600), Some(111)).unwrap();
        let row = db.feed(10, false).unwrap().into_iter().find(|r| r.id == "a").unwrap();
        assert_eq!((row.published_at, row.published_approx, row.duration_s), (111, false, Some(600)));
        assert_eq!(db.last_fetch().unwrap(), None);
    }

    #[test]
    fn removing_a_channel_removes_its_videos() {
        let mut db = Db::open_in_memory().unwrap();
        db.add_channel(&ch("UCsBjURrPoezykLs9EqgamOA", "Fireship"), 0).unwrap();
        db.upsert_videos(&[video("a", 1, false)], 0).unwrap();
        assert!(db.remove_channel("UCsBjURrPoezykLs9EqgamOA").unwrap());
        assert!(db.feed(10, true).unwrap().is_empty());
    }
}
