//! YouTube's public channel feed: `https://www.youtube.com/feeds/videos.xml?channel_id=UC…`.
//! Atom with `yt:` and `media:` extensions; it lists the ~15 newest uploads, Shorts included
//! (their link is `/shorts/<id>`).

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::model::VideoEntry;

pub fn feed_url(channel_id: &str) -> String {
    format!("https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}")
}

#[derive(Debug)]
pub struct ParsedFeed {
    pub channel_title: Option<String>,
    pub entries: Vec<VideoEntry>,
}

#[derive(Deserialize)]
struct Feed {
    title: Option<String>,
    #[serde(rename = "entry", default)]
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "videoId")]
    video_id: String,
    #[serde(rename = "channelId")]
    channel_id: String,
    title: String,
    #[serde(default)]
    link: Vec<Link>,
    published: String,
    #[serde(rename = "group")]
    media: Option<MediaGroup>,
}

#[derive(Deserialize)]
struct Link {
    #[serde(rename = "@href")]
    href: String,
}

#[derive(Deserialize)]
struct MediaGroup {
    #[serde(rename = "thumbnail")]
    thumbnail: Option<Thumbnail>,
}

#[derive(Deserialize)]
struct Thumbnail {
    #[serde(rename = "@url")]
    url: String,
}

/// The deserializer matches local names (`yt:videoId` is `videoId`) and ignores the root's name,
/// so check the root here: an HTML error page must not parse as an empty feed.
fn check_root(xml: &str) -> Result<()> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    loop {
        match reader.read_event().context("parsing channel RSS")? {
            Event::Start(e) | Event::Empty(e) => {
                return match e.local_name().as_ref() {
                    "feed" => Ok(()),
                    other => bail!("expected an Atom <feed>, got <{other}>"),
                };
            }
            Event::DocType(_) => bail!("expected an Atom feed, got an HTML page"),
            Event::Eof => bail!("empty response"),
            _ => {}
        }
    }
}

pub fn parse(xml: &str) -> Result<ParsedFeed> {
    check_root(xml)?;
    let feed: Feed = quick_xml::de::from_str(xml).context("parsing channel RSS")?;
    let entries = feed
        .entries
        .into_iter()
        .map(|e| {
            let published_at = chrono::DateTime::parse_from_rfc3339(&e.published)
                .with_context(|| format!("bad <published> {:?} for {}", e.published, e.video_id))?
                .timestamp();
            let is_short = e.link.iter().any(|l| l.href.contains("/shorts/"));
            Ok(VideoEntry {
                thumb_url: e.media.and_then(|m| m.thumbnail).map(|t| t.url),
                id: e.video_id,
                channel_id: e.channel_id,
                title: e.title,
                published_at,
                published_approx: false,
                duration_s: None,
                is_short,
            })
        })
        .collect::<Result<_>>()?;
    Ok(ParsedFeed { channel_title: feed.title, entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture() {
        let feed = parse(include_str!("../tests/fixtures/rss_fireship.xml")).unwrap();
        assert_eq!(feed.channel_title.as_deref(), Some("Fireship"));
        assert_eq!(feed.entries.len(), 3);

        let first = &feed.entries[0];
        assert_eq!(first.id, "QZVzuq28eFw");
        // The feed-level <yt:channelId> drops the "UC"; the entries' keep it.
        assert_eq!(first.channel_id, "UCsBjURrPoezykLs9EqgamOA");
        assert_eq!(first.title, "OpenAI’s lead engineer was secretly a spy...");
        assert_eq!(first.published_at, 1_792_000_090); // 2026-10-14T17:48:10Z
        assert!(!first.published_approx && !first.is_short);
        assert_eq!(first.thumb_url.as_deref(), Some("https://i2.ytimg.com/vi/QZVzuq28eFw/hqdefault.jpg"));

        let second = &feed.entries[1];
        assert_eq!(second.title, "Every app on Earth is now open source... whether it likes it or not");
        assert_eq!(second.published_at, 1_791_832_523); // 2026-10-12T19:15:23Z
        assert!(feed.entries.iter().all(|e| !e.is_short));
    }

    #[test]
    fn marks_shorts_by_link() {
        let feed = parse(include_str!("../tests/fixtures/rss_nippard.xml")).unwrap();
        assert_eq!(feed.channel_title.as_deref(), Some("Jeff Nippard"));
        let ids: Vec<_> = feed.entries.iter().map(|e| (e.id.as_str(), e.is_short)).collect();
        assert_eq!(ids, [("JVO00Ulr5BE", true), ("H6SiAe-VObA", false)]);
        assert_eq!(feed.entries[0].title, "The Best & Worst Peptides (Ranked)");
    }

    #[test]
    fn empty_channel() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Quiet</title></feed>"#;
        let feed = parse(xml).unwrap();
        assert!(feed.entries.is_empty());
    }

    #[test]
    fn rejects_html_error_pages() {
        assert!(parse("<!DOCTYPE html><html><title>Error 404 (Not Found)!!1</title></html>").is_err());
    }
}
