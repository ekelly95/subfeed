//! SponsorBlock (sponsor.ajay.app, community data): parts of a video to skip automatically, which
//! are sponsor reads, unpaid self-promotion and "like and subscribe" reminders.
//!
//! Privately: the request names only the first 4 hex digits of the video id's SHA-256, and the
//! server answers with every video in that bucket; the match happens here. A missing or failed
//! answer just means nothing is skipped.

use anyhow::{Result, bail};
use serde::Deserialize;

pub const CATEGORIES: &[&str] = &["sponsor", "selfpromo", "interaction"];

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub category: String,
    /// What the "Skipped …" note calls it.
    pub label: String,
}

pub fn label(category: &str) -> &'static str {
    match category {
        "sponsor" => "sponsor",
        "selfpromo" => "self-promotion",
        "interaction" => "subscribe reminder",
        _ => "segment",
    }
}

/// The first 4 hex digits of the video id's SHA-256: all the server learns.
pub fn hash_prefix(video_id: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, video_id.as_bytes());
    digest.as_ref()[..2].iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Deserialize)]
struct Video {
    #[serde(rename = "videoID")]
    video_id: String,
    #[serde(default)]
    segments: Vec<RawSegment>,
}

#[derive(Deserialize)]
struct RawSegment {
    segment: (f64, f64),
    category: String,
    #[serde(rename = "actionType", default)]
    action_type: String,
}

/// This video's skippable segments from a bucket answer, in order, without zero-length ones.
pub fn parse(json: &str, video_id: &str) -> Result<Vec<Segment>> {
    let videos: Vec<Video> = serde_json::from_str(json)?;
    let mut segments: Vec<Segment> = videos
        .into_iter()
        .filter(|v| v.video_id == video_id)
        .flat_map(|v| v.segments)
        .filter(|s| s.action_type == "skip" && CATEGORIES.contains(&s.category.as_str()) && s.segment.1 > s.segment.0 + 0.5)
        .map(|s| Segment { start: s.segment.0, end: s.segment.1, label: label(&s.category).into(), category: s.category })
        .collect();
    segments.sort_by(|a, b| a.start.total_cmp(&b.start));
    Ok(segments)
}

pub fn fetch(video_id: &str) -> Result<Vec<Segment>> {
    let categories = serde_json::to_string(CATEGORIES)?;
    let url = format!("https://sponsor.ajay.app/api/skipSegments/{}", hash_prefix(video_id));
    let mut resp = crate::thumbs::agent()
        .get(&url)
        .query("categories", &categories)
        .query("actionTypes", "[\"skip\"]")
        .call()?;
    match resp.status().as_u16() {
        // No video in the bucket has segments.
        404 => Ok(Vec::new()),
        200 => parse(&resp.body_mut().read_to_string()?, video_id),
        code => bail!("SponsorBlock answered HTTP {code}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_is_the_first_four_hex_digits_of_sha256() {
        // sha256("NiCgcUTL9_Q") starts with b8a7 (checked with sha256sum).
        assert_eq!(hash_prefix("NiCgcUTL9_Q"), "b8a7");
    }

    #[test]
    fn keeps_only_this_videos_skips() {
        let json = r#"[
          {"videoID": "someoneElse", "segments": [{"segment": [1, 20], "category": "sponsor", "actionType": "skip"}]},
          {"videoID": "NiCgcUTL9_Q", "segments": [
            {"segment": [1954.529, 1973.945], "category": "interaction", "actionType": "skip"},
            {"segment": [959.599, 1047.019], "category": "sponsor", "actionType": "skip"},
            {"segment": [10, 10.2], "category": "sponsor", "actionType": "skip"},
            {"segment": [100, 140], "category": "sponsor", "actionType": "mute"},
            {"segment": [0, 30], "category": "intro", "actionType": "skip"}
          ]}
        ]"#;
        let segments = parse(json, "NiCgcUTL9_Q").unwrap();
        let got: Vec<(f64, &str)> = segments.iter().map(|s| (s.start, s.label.as_str())).collect();
        assert_eq!(got, [(959.599, "sponsor"), (1954.529, "subscribe reminder")]);
        assert!(parse("[]", "x").unwrap().is_empty());
    }
}
