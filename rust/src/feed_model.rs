//! `FeedModel`: the video grid's list model, and `ChannelModel`: the sidebar's.
//!
//! Both read SQLite directly. Network work (refresh, thumbnails) runs on background threads and
//! hands results back to the Qt thread with `qt_thread().queue`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        #[qobject]
        type QAbstractListModel;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(bool, refreshing)]
        #[qproperty(QString, status)]
        #[qproperty(i32, channel_count, cxx_name = "channelCount")]
        /// The newest unwatched video, shown large above the grid (and left out of it).
        /// JSON with the row roles plus `hero` (a 1280×720 image); "" when there is none.
        #[qproperty(QString, hero_json, cxx_name = "heroJson")]
        /// JSON array of started-but-unfinished videos, most recent first.
        #[qproperty(QString, continue_json, cxx_name = "continueJson")]
        /// When channels were last checked: "4 minutes ago", "yesterday", or "never".
        #[qproperty(QString, last_refresh, cxx_name = "lastRefresh")]
        type FeedModel = super::FeedModelRust;

        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(i32, total_unwatched, cxx_name = "totalUnwatched")]
        #[qproperty(i32, total_unwatched_shorts, cxx_name = "totalUnwatchedShorts")]
        type ChannelModel = super::ChannelModelRust;
    }

    impl cxx_qt::Threading for FeedModel {}

    extern "RustQt" {
        /// Emitted after anything that changes unwatched counts (watched, hide, refresh).
        #[qsignal]
        fn changed(self: Pin<&mut FeedModel>);

        #[qinvokable]
        fn reload(self: Pin<&mut FeedModel>);
        /// `shorts`: show only Shorts (the Shorts view, which has no hero or "continue" row).
        #[qinvokable]
        #[cxx_name = "setFilter"]
        fn set_filter(self: Pin<&mut FeedModel>, channel_id: &QString, search: &QString, unwatched_only: bool, shorts: bool);
        #[qinvokable]
        #[cxx_name = "setWatched"]
        fn set_watched(self: Pin<&mut FeedModel>, row: i32, watched: bool);
        /// Hides the video and removes its row. Returns the video id, for undo.
        #[qinvokable]
        fn hide(self: Pin<&mut FeedModel>, row: i32) -> QString;
        #[qinvokable]
        fn unhide(self: Pin<&mut FeedModel>, video_id: &QString);
        /// Fetches all channels in the background, then reloads.
        #[qinvokable]
        fn refresh(self: Pin<&mut FeedModel>);
        /// Row of a video id, or -1.
        #[qinvokable]
        #[cxx_name = "rowOf"]
        fn row_of(self: &FeedModel, video_id: &QString) -> i32;
        /// One row as JSON (the same keys as the roles), or "" when out of range.
        #[qinvokable]
        fn get(self: &FeedModel, row: i32) -> QString;
        /// Up to `count` unwatched videos after `video_id` in feed order, as a JSON array.
        #[qinvokable]
        #[cxx_name = "upNext"]
        fn up_next(self: &FeedModel, video_id: &QString, count: i32) -> QString;
        /// Hides the hero video. Returns its id, for undo.
        #[qinvokable]
        #[cxx_name = "hideHero"]
        fn hide_hero(self: Pin<&mut FeedModel>) -> QString;
        #[qinvokable]
        #[cxx_name = "setHeroWatched"]
        fn set_hero_watched(self: Pin<&mut FeedModel>, watched: bool);
        /// Looks the video up in the background so opening it is quick (see `lookup.rs`).
        #[qinvokable]
        fn prefetch(self: &FeedModel, video_id: &QString);

        #[qinvokable]
        #[cxx_override]
        fn data(self: &FeedModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &FeedModel, parent: &QModelIndex) -> i32;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &FeedModel) -> QHash_i32_QByteArray;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut FeedModel>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QVector_i32);
        #[inherit]
        fn index(self: &FeedModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut FeedModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut FeedModel>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(self: Pin<&mut FeedModel>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut FeedModel>);
    }

    extern "RustQt" {
        #[qinvokable]
        fn reload(self: Pin<&mut ChannelModel>);
        /// Unfollows a channel; its videos and watch state go too. Returns whether it existed.
        #[qinvokable]
        fn remove(self: Pin<&mut ChannelModel>, channel_id: &QString) -> bool;

        #[qinvokable]
        #[cxx_override]
        fn data(self: &ChannelModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &ChannelModel, parent: &QModelIndex) -> i32;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &ChannelModel) -> QHash_i32_QByteArray;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut ChannelModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut ChannelModel>);
    }
}

use std::collections::HashSet;
use std::path::PathBuf;
use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector};

use crate::db::{self, ChannelSummary, Db, FeedFilter, FeedRow};
use crate::lookup::Lookups;
use crate::refresh::{self, HttpSource, Options};
use crate::{format, thumbs};

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn open_db() -> anyhow::Result<Db> {
    Db::open(&db::default_path())
}

fn role_hash(names: &[&str]) -> QHash<QHashPair_i32_QByteArray> {
    let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
    for (i, name) in names.iter().enumerate() {
        roles.insert(USER_ROLE + i as i32, QByteArray::from(*name));
    }
    roles
}

/// Qt::UserRole: custom roles start here so they don't collide with Qt's own.
const USER_ROLE: i32 = 256;

// ---------------------------------------------------------------------------------------------
// FeedModel

const FEED_ROLES: &[&str] =
    &["videoId", "title", "channelTitle", "channelId", "age", "duration", "thumbnail", "watched", "progress", "isShort"];
const ROLE_THUMBNAIL: i32 = USER_ROLE + 6;
const ROLE_WATCHED: i32 = USER_ROLE + 7;

struct Row {
    video: FeedRow,
    age: QString,
    duration: QString,
    thumbnail: QString,
}

pub struct FeedModelRust {
    refreshing: bool,
    status: QString,
    channel_count: i32,
    hero_json: QString,
    continue_json: QString,
    last_refresh: QString,
    hero: Option<FeedRow>,

    db: Option<Db>,
    filter: FeedFilter,
    rows: Vec<Row>,
    thumb_dir: PathBuf,
    /// Thumbnails being downloaded, so a reload doesn't start the same download twice.
    thumbs_in_flight: HashSet<String>,
}

impl Default for FeedModelRust {
    fn default() -> Self {
        Self {
            refreshing: false,
            status: QString::default(),
            channel_count: 0,
            hero_json: QString::default(),
            continue_json: QString::from("[]"),
            last_refresh: QString::default(),
            hero: None,
            db: None,
            filter: FeedFilter::default(),
            rows: Vec::new(),
            thumb_dir: thumbs::cache_dir(),
            thumbs_in_flight: HashSet::new(),
        }
    }
}

impl qobject::FeedModel {
    pub fn reload(mut self: Pin<&mut Self>) {
        if self.db.is_none() {
            match open_db() {
                Ok(db) => self.as_mut().rust_mut().db = Some(db),
                Err(e) => {
                    self.as_mut().set_status(QString::from(&format!("Can't open the database: {e:#}")));
                    return;
                }
            }
        }
        let db = self.db.as_ref().expect("opened above");
        let loaded = (|| -> anyhow::Result<_> {
            let plain = self.filter.search.is_none() && !self.filter.shorts_only;
            let continuing = if plain { db.continue_watching(3)? } else { Vec::new() };
            Ok((db.query_feed(&self.filter)?, db.channels()?.len() as i32, continuing, db.last_fetch()?))
        })();
        let (mut rows, channels, continuing, last_fetch) = match loaded {
            Ok(v) => v,
            Err(e) => {
                self.as_mut().set_status(QString::from(&format!("Database error: {e:#}")));
                return;
            }
        };
        let now = now();
        // The hero: newest unwatched video, unless searching or in the Shorts view.
        let hero = if self.filter.search.is_none() && !self.filter.shorts_only {
            rows.iter().position(|r| !r.watched).map(|i| rows.remove(i))
        } else {
            None
        };
        let dir = self.thumb_dir.clone();
        let rows: Vec<Row> = rows
            .into_iter()
            .map(|video| Row {
                age: QString::from(&format::age(video.published_at, video.published_approx, now)),
                duration: QString::from(&video.duration_s.map(format::duration).unwrap_or_default()),
                thumbnail: QString::from(&thumbs::cached_url(&dir, &video.id).unwrap_or_default()),
                video,
            })
            .collect();
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().rows = rows;
            self.as_mut().end_reset_model();
        }
        self.as_mut().set_channel_count(channels);
        let continuing: Vec<_> = continuing.iter().map(|v| row_json(v, &dir, now)).collect();
        self.as_mut().set_continue_json(QString::from(&serde_json::Value::Array(continuing).to_string()));
        let refreshed = last_fetch.map(|t| format::age(t, false, now)).unwrap_or_else(|| "never".into());
        self.as_mut().set_last_refresh(QString::from(&refreshed));
        self.as_mut().rust_mut().hero = hero;
        self.as_mut().update_hero();
        self.fetch_thumbnails();
    }

    fn update_hero(mut self: Pin<&mut Self>) {
        let dir = self.thumb_dir.clone();
        let json = match &self.hero {
            Some(v) => {
                let mut j = row_json(v, &dir, now());
                j["hero"] = thumbs::cached_hero_url(&dir, &v.id).map(Into::into).unwrap_or_else(|| j["thumbnail"].clone());
                j.to_string()
            }
            None => String::new(),
        };
        self.as_mut().set_hero_json(QString::from(&json));
        // Fetch the large frame once; fall back to the normal thumbnail if the video has none.
        let Some(id) = self.hero.as_ref().map(|v| v.id.clone()) else { return };
        let Some(path) = thumbs::hero_path_for(&dir, &id).filter(|p| !p.exists()) else { return };
        if !self.as_mut().rust_mut().thumbs_in_flight.insert(format!("{id}.max")) {
            return;
        }
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let ok = thumbs::download(&thumbs::agent(), &thumbs::hero_source_url(&id), &path).is_ok();
            let _ = qt.queue(move |mut model| {
                model.as_mut().rust_mut().thumbs_in_flight.remove(&format!("{id}.max"));
                if ok && model.hero.as_ref().is_some_and(|h| h.id == id) {
                    model.update_hero();
                }
            });
        });
    }

    /// Downloads missing thumbnails for the current rows on a background thread.
    fn fetch_thumbnails(mut self: Pin<&mut Self>) {
        let todo: Vec<(String, String)> = self
            .rows
            .iter()
            .filter(|r| r.thumbnail.is_empty() && !self.thumbs_in_flight.contains(&r.video.id))
            .filter_map(|r| Some((r.video.id.clone(), r.video.thumb_url.clone()?)))
            .collect();
        if todo.is_empty() {
            return;
        }
        let ids: Vec<String> = todo.iter().map(|(id, _)| id.clone()).collect();
        self.as_mut().rust_mut().thumbs_in_flight.extend(ids.iter().cloned());
        let (qt, dir) = (self.qt_thread(), self.thumb_dir.clone());
        std::thread::spawn(move || {
            thumbs::fetch_missing(&dir, todo, 6, &|id, path| {
                let (id, url) = (id.to_string(), format!("file://{}", path.display()));
                let _ = qt.queue(move |model| model.thumbnail_ready(&id, &url));
            });
            let _ = qt.queue(move |mut model| {
                for id in &ids {
                    model.as_mut().rust_mut().thumbs_in_flight.remove(id);
                }
            });
        });
    }

    fn thumbnail_ready(mut self: Pin<&mut Self>, id: &str, url: &str) {
        let Some(row) = self.rows.iter().position(|r| r.video.id == id) else { return };
        self.as_mut().rust_mut().rows[row].thumbnail = QString::from(url);
        self.emit_row_changed(row as i32, ROLE_THUMBNAIL);
    }

    fn emit_row_changed(mut self: Pin<&mut Self>, row: i32, role: i32) {
        let index = self.index(row, 0, &QModelIndex::default());
        let mut roles = QVector::<i32>::default();
        roles.append(role);
        self.as_mut().data_changed(&index, &index, &roles);
    }

    pub fn set_filter(mut self: Pin<&mut Self>, channel_id: &QString, search: &QString, unwatched_only: bool, shorts: bool) {
        let non_empty = |s: &QString| Some(s.to_string()).filter(|s| !s.trim().is_empty());
        self.as_mut().rust_mut().filter = FeedFilter {
            channel_id: non_empty(channel_id),
            search: non_empty(search),
            unwatched_only,
            shorts_only: shorts,
            ..FeedFilter::default()
        };
        self.reload();
    }

    pub fn set_watched(mut self: Pin<&mut Self>, row: i32, watched: bool) {
        let Some(id) = self.rows.get(row as usize).map(|r| r.video.id.clone()) else { return };
        if let Some(db) = &self.db
            && let Err(e) = db.set_watched(&id, watched, now())
        {
            self.as_mut().set_status(QString::from(&format!("Couldn't save: {e:#}")));
            return;
        }
        // The row stays put even under "unwatched only", so the grid doesn't jump; it goes
        // on the next reload.
        self.as_mut().rust_mut().rows[row as usize].video.watched = watched;
        self.as_mut().emit_row_changed(row, ROLE_WATCHED);
        self.changed();
    }

    pub fn hide(mut self: Pin<&mut Self>, row: i32) -> QString {
        let Some(id) = self.rows.get(row as usize).map(|r| r.video.id.clone()) else { return QString::default() };
        if let Some(db) = &self.db
            && let Err(e) = db.set_hidden(&id, true)
        {
            self.as_mut().set_status(QString::from(&format!("Couldn't save: {e:#}")));
            return QString::default();
        }
        unsafe {
            self.as_mut().begin_remove_rows(&QModelIndex::default(), row, row);
            self.as_mut().rust_mut().rows.remove(row as usize);
            self.as_mut().end_remove_rows();
        }
        self.changed();
        QString::from(&id)
    }

    pub fn unhide(mut self: Pin<&mut Self>, video_id: &QString) {
        if let Some(db) = &self.db {
            let _ = db.set_hidden(&video_id.to_string(), false);
        }
        self.as_mut().reload();
        self.changed();
    }

    pub fn get(&self, row: i32) -> QString {
        let Some(r) = self.rows.get(row as usize) else { return QString::default() };
        QString::from(&row_json(&r.video, &self.thumb_dir, now()).to_string())
    }

    pub fn up_next(&self, video_id: &QString, count: i32) -> QString {
        let id = video_id.to_string();
        let order: Vec<&FeedRow> = self.hero.iter().chain(self.rows.iter().map(|r| &r.video)).collect();
        let start = order.iter().position(|v| v.id == id).map_or(0, |i| i + 1);
        let now = now();
        let next: Vec<_> = order[start.min(order.len())..]
            .iter()
            .filter(|v| !v.watched && v.id != id)
            .take(count.max(0) as usize)
            .map(|v| row_json(v, &self.thumb_dir, now))
            .collect();
        QString::from(&serde_json::Value::Array(next).to_string())
    }

    pub fn hide_hero(mut self: Pin<&mut Self>) -> QString {
        let Some(id) = self.hero.as_ref().map(|h| h.id.clone()) else { return QString::default() };
        if let Some(db) = &self.db {
            let _ = db.set_hidden(&id, true);
        }
        self.as_mut().reload();
        self.changed();
        QString::from(&id)
    }

    pub fn set_hero_watched(mut self: Pin<&mut Self>, watched: bool) {
        let Some(id) = self.hero.as_ref().map(|h| h.id.clone()) else { return };
        if let Some(db) = &self.db {
            let _ = db.set_watched(&id, watched, now());
        }
        self.as_mut().reload();
        self.changed();
    }

    pub fn prefetch(&self, video_id: &QString) {
        let id = video_id.to_string();
        if thumbs::is_video_id(&id) {
            Lookups::global().prefetch(&id);
        }
    }

    pub fn row_of(&self, video_id: &QString) -> i32 {
        let id = video_id.to_string();
        self.rows.iter().position(|r| r.video.id == id).map_or(-1, |i| i as i32)
    }

    pub fn refresh(mut self: Pin<&mut Self>) {
        if self.refreshing {
            return;
        }
        self.as_mut().set_refreshing(true);
        self.as_mut().set_status(QString::from("Checking your channels…"));
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let result = open_db().and_then(|mut db| {
                let s = refresh::refresh(&mut db, &HttpSource::new(), &Options::default(), now())?;
                Ok(refresh_message(&s))
            });
            let _ = qt.queue(move |mut model| {
                let message = result.unwrap_or_else(|e| format!("Refresh failed: {e:#}"));
                model.as_mut().set_refreshing(false);
                model.as_mut().set_status(QString::from(&message));
                model.as_mut().reload();
                model.changed();
            });
        });
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(r) = self.rows.get(index.row() as usize) else { return QVariant::default() };
        let v = &r.video;
        match role - USER_ROLE {
            0 => QVariant::from(&QString::from(&v.id)),
            1 => QVariant::from(&QString::from(&v.title)),
            2 => QVariant::from(&QString::from(&v.channel_title)),
            3 => QVariant::from(&QString::from(&v.channel_id)),
            4 => QVariant::from(&r.age),
            5 => QVariant::from(&r.duration),
            6 => QVariant::from(&r.thumbnail),
            7 => QVariant::from(&v.watched),
            8 => QVariant::from(&progress(v)),
            9 => QVariant::from(&v.is_short),
            _ => QVariant::default(),
        }
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rows.len() as i32
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        role_hash(FEED_ROLES)
    }
}

/// A row as the JSON QML gets from `get`, `heroJson`, `continueJson` and `upNext`.
fn row_json(v: &FeedRow, dir: &std::path::Path, now: i64) -> serde_json::Value {
    let left = v.duration_s.map(|d| format::duration((d as f64 - v.position_s).max(0.0) as i64));
    serde_json::json!({
        "videoId": v.id,
        "title": v.title,
        "channelTitle": v.channel_title,
        "channelId": v.channel_id,
        "age": format::age(v.published_at, v.published_approx, now),
        "duration": v.duration_s.map(format::duration).unwrap_or_default(),
        "thumbnail": thumbs::cached_url(dir, &v.id).unwrap_or_default(),
        "watched": v.watched,
        "progress": progress(v),
        "isShort": v.is_short,
        "left": left.unwrap_or_default(),
    })
}

/// Fraction watched, for the progress bar under a thumbnail.
fn progress(v: &FeedRow) -> f64 {
    match v.duration_s {
        Some(d) if d > 0 => (v.position_s / d as f64).clamp(0.0, 1.0),
        _ => 0.0,
    }
}

fn refresh_message(s: &refresh::Summary) -> String {
    let videos = match s.new_videos {
        0 => "No new videos".to_string(),
        1 => "1 new video".to_string(),
        n => format!("{n} new videos"),
    };
    let mut parts = vec![videos];
    if s.failed > 0 {
        parts.push(format!("{} channels failed", s.failed));
    }
    if s.rss_outage {
        parts.push("YouTube's RSS is down, so dates are approximate".into());
    }
    parts.join(" · ")
}

// ---------------------------------------------------------------------------------------------
// ChannelModel

const CHANNEL_ROLES: &[&str] =
    &["channelId", "title", "unwatched", "unwatchedShorts", "videoCount", "latest", "problem"];

#[derive(Default)]
pub struct ChannelModelRust {
    total_unwatched: i32,
    total_unwatched_shorts: i32,
    db: Option<Db>,
    rows: Vec<ChannelSummary>,
}

impl qobject::ChannelModel {
    pub fn reload(mut self: Pin<&mut Self>) {
        if self.db.is_none() {
            self.as_mut().rust_mut().db = open_db().ok();
        }
        let rows = self.db.as_ref().and_then(|db| db.channel_summaries().ok()).unwrap_or_default();
        let total = rows.iter().map(|c| c.unwatched).sum::<i64>() as i32;
        let total_shorts = rows.iter().map(|c| c.unwatched_shorts).sum::<i64>() as i32;
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().rows = rows;
            self.as_mut().end_reset_model();
        }
        self.as_mut().set_total_unwatched(total);
        self.set_total_unwatched_shorts(total_shorts);
    }

    pub fn remove(self: Pin<&mut Self>, channel_id: &QString) -> bool {
        let removed = self.db.as_ref().is_some_and(|db| db.remove_channel(&channel_id.to_string()).unwrap_or(false));
        self.reload();
        removed
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(c) = self.rows.get(index.row() as usize) else { return QVariant::default() };
        match role - USER_ROLE {
            0 => QVariant::from(&QString::from(&c.id)),
            1 => QVariant::from(&QString::from(&c.title)),
            2 => QVariant::from(&(c.unwatched as i32)),
            3 => QVariant::from(&(c.unwatched_shorts as i32)),
            4 => QVariant::from(&(c.video_count as i32)),
            // "2 days ago", or "" before the first refresh.
            5 => QVariant::from(&QString::from(
                &c.latest_published.map(|t| format::age(t, false, now())).unwrap_or_default(),
            )),
            6 => QVariant::from(&QString::from(c.last_error.as_deref().unwrap_or(""))),
            _ => QVariant::default(),
        }
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rows.len() as i32
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        role_hash(CHANNEL_ROLES)
    }
}
