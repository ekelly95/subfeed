//! Turning a video id into something mpv can play: one `yt-dlp -J` call gives the metadata
//! (description, chapters) and every format; we pick a video and an audio stream for this laptop.
//!
//! Picking ourselves (rather than mpv's ytdl hook) means one request to YouTube instead of two,
//! and the choice follows the power state: on battery or power-saver, at most 720p30; otherwise
//! up to 1080p60. Codecs the GPU decodes come first (AV1, VP9, H.264), and HDR is skipped.
//! "720p" means the short side, as YouTube labels it, so a vertical Short gets 720×1280.

use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ytdlp;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chapter {
    pub start_time: f64,
    pub title: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Format {
    pub format_id: String,
    pub url: Option<String>,
    #[serde(default)]
    pub protocol: String,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub tbr: Option<f64>,
    pub dynamic_range: Option<String>,
    pub language_preference: Option<i32>,
    #[serde(default)]
    pub http_headers: std::collections::BTreeMap<String, String>,
}

impl Format {
    fn has(codec: &Option<String>) -> bool {
        codec.as_deref().is_some_and(|c| c != "none")
    }
    fn is_video_only(&self) -> bool {
        Self::has(&self.vcodec) && !Self::has(&self.acodec)
    }
    fn is_audio_only(&self) -> bool {
        Self::has(&self.acodec) && !Self::has(&self.vcodec)
    }
    fn is_muxed(&self) -> bool {
        Self::has(&self.vcodec) && Self::has(&self.acodec)
    }
    /// Plain HTTPS progressive/DASH streams only; HLS and storyboards are skipped.
    fn playable(&self) -> bool {
        self.url.is_some() && self.protocol == "https"
    }
    /// The resolution YouTube would name it by ("720p"): the short side, so a vertical
    /// 720×1280 stream counts as 720p, not 1280p.
    fn lines(&self) -> Option<u32> {
        match (self.width, self.height) {
            (Some(w), Some(h)) => Some(w.min(h)),
            (_, h) => h,
        }
    }
    fn is_sdr(&self) -> bool {
        self.dynamic_range.as_deref().is_none_or(|d| d == "SDR")
    }
    pub fn codec_name(&self) -> &'static str {
        let v = self.vcodec.as_deref().unwrap_or("");
        if v.starts_with("av01") {
            "AV1"
        } else if v.starts_with("vp9") || v.starts_with("vp09") {
            "VP9"
        } else if v.starts_with("avc1") {
            "H.264"
        } else {
            "video"
        }
    }
    /// Lowercase id matching `SystemInfo.decodableCodecs`: "av1", "vp9", "h264".
    fn codec_id(&self) -> &'static str {
        match self.codec_name() {
            "AV1" => "av1",
            "VP9" => "vp9",
            "H.264" => "h264",
            _ => "",
        }
    }

    /// Higher is better. Codecs the GPU decodes beat ones it doesn't (most efficient first:
    /// AV1, VP9, H.264). With no VA-API information, prefer what is cheapest to decode
    /// elsewhere: VP9, then H.264, then AV1.
    fn codec_rank(&self, decodable: &[String]) -> u8 {
        let id = self.codec_id();
        if decodable.is_empty() {
            return match id {
                "vp9" => 3,
                "h264" => 2,
                "av1" => 1,
                _ => 0,
            };
        }
        let base = match id {
            "av1" => 3,
            "vp9" => 2,
            "h264" => 1,
            _ => 0,
        };
        if decodable.iter().any(|c| c == id) { base + 10 } else { base }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub channel: Option<String>,
    pub duration: Option<f64>,
    pub timestamp: Option<i64>,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub chapters: Vec<Chapter>,
    pub live_status: Option<String>,
    #[serde(default)]
    pub formats: Vec<Format>,
    /// The video's spoken language ("en-US"), when YouTube says.
    pub language: Option<String>,
    /// Captions by language code: uploaded by the channel...
    #[serde(default, deserialize_with = "null_as_default")]
    pub subtitles: BTreeMap<String, Vec<SubtitleFormat>>,
    /// ...and YouTube's automatic ones ("en-orig" is the original-language track; the rest are
    /// machine translations of it).
    #[serde(default, deserialize_with = "null_as_default")]
    pub automatic_captions: BTreeMap<String, Vec<SubtitleFormat>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubtitleFormat {
    pub ext: String,
    pub url: String,
    pub name: Option<String>,
}

/// The caption track to offer for a video.
#[derive(Debug, Clone, PartialEq)]
pub struct Caption {
    pub url: String,
    /// "English", or "English (auto)" for YouTube's automatic captions.
    pub label: String,
}

/// "en" from "en-US", "en_GB" or "en-orig".
fn primary_language(code: &str) -> String {
    code.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase()
}

/// Picks captions for someone reading `ui_lang` ("en_US"): the channel's own captions in their
/// language, then the channel's own in the video's language, then YouTube's automatic captions
/// in the original language when that's theirs, then automatic ones translated to theirs.
/// WebVTT is preferred, then SRT (both play in mpv).
pub fn pick_captions(info: &VideoInfo, ui_lang: &str) -> Option<Caption> {
    let ui = primary_language(ui_lang);
    let spoken = info.language.as_deref().map(primary_language);
    let track = |formats: &[SubtitleFormat]| -> Option<(String, Option<String>)> {
        ["vtt", "srt"]
            .iter()
            .find_map(|ext| formats.iter().find(|f| f.ext == *ext))
            .map(|f| (f.url.clone(), f.name.clone()))
    };
    let find = |map: &BTreeMap<String, Vec<SubtitleFormat>>, lang: &str, exact_ui: Option<&str>| {
        // The exact regional code first ("en-US" for an en_US desktop), then any in the language.
        exact_ui
            .and_then(|code| map.get(code))
            .or_else(|| map.iter().find(|(k, _)| !k.ends_with("-orig") && primary_language(k) == lang).map(|(_, v)| v))
            .and_then(|v| track(v))
    };
    let ui_exact = ui_lang.replace('_', "-");
    let manual = find(&info.subtitles, &ui, Some(&ui_exact))
        .or_else(|| spoken.as_deref().filter(|s| *s != ui).and_then(|s| find(&info.subtitles, s, None)));
    if let Some((url, name)) = manual {
        return Some(Caption { url, label: name.unwrap_or_else(|| "Captions".into()) });
    }
    let auto = (spoken.as_deref().is_none_or(|s| s == ui))
        .then(|| info.automatic_captions.get(&format!("{ui}-orig")).and_then(|v| track(v)))
        .flatten()
        .or_else(|| info.automatic_captions.get(&ui).and_then(|v| track(v)));
    auto.map(|(url, name)| {
        let name = name.unwrap_or_else(|| "Captions".into()).replace(" (Original)", "");
        Caption { url, label: format!("{name} (auto)") }
    })
}

fn null_as_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Policy {
    pub max_height: u32,
    pub max_fps: f64,
}

impl Policy {
    /// On battery or in power-saver mode.
    pub const LOW_POWER: Policy = Policy { max_height: 720, max_fps: 30.0 };
    pub const AC: Policy = Policy { max_height: 1080, max_fps: 60.0 };

    pub fn for_power(on_battery: bool, power_saver: bool) -> Self {
        if on_battery || power_saver { Self::LOW_POWER } else { Self::AC }
    }
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub video: Format,
    /// None when `video` already carries audio.
    pub audio: Option<Format>,
}

impl Selection {
    /// "720p · AV1".
    pub fn label(&self) -> String {
        match self.video.lines() {
            Some(h) => format!("{h}p · {}", self.video.codec_name()),
            None => self.video.codec_name().to_string(),
        }
    }
}

/// `decodable`: the codecs the GPU decodes in hardware (empty when unknown).
pub fn select(info: &VideoInfo, policy: Policy, decodable: &[String]) -> Result<Selection> {
    let fits = |f: &&Format| {
        f.lines().is_some_and(|h| h <= policy.max_height) && f.fps.unwrap_or(30.0) <= policy.max_fps + 0.5 && f.is_sdr()
    };
    let video = info
        .formats
        .iter()
        .filter(|f| f.playable() && f.is_video_only())
        .filter(fits)
        .max_by(|a, b| {
            // A hardware-decodable codec matters more than one step of resolution.
            let key = |f: &Format| (f.codec_rank(decodable) >= 10, f.lines(), f.codec_rank(decodable));
            key(a).cmp(&key(b)).then(a.fps.unwrap_or(0.0).total_cmp(&b.fps.unwrap_or(0.0)))
        });
    // The original-language track (yt-dlp marks it with the highest preference), Opus first.
    let audio = info.formats.iter().filter(|f| f.playable() && f.is_audio_only()).max_by(|a, b| {
        let key = |f: &Format| (f.language_preference.unwrap_or(0), f.acodec.as_deref() == Some("opus"));
        key(a).cmp(&key(b)).then(a.tbr.unwrap_or(0.0).total_cmp(&b.tbr.unwrap_or(0.0)))
    });
    if let (Some(v), Some(a)) = (video, audio) {
        return Ok(Selection { video: v.clone(), audio: Some(a.clone()) });
    }
    // Old or unusual videos: a single muxed stream.
    let muxed = info
        .formats
        .iter()
        .filter(|f| f.playable() && f.is_muxed())
        .max_by_key(|f| (fits(f), f.lines().unwrap_or(0)));
    match muxed {
        Some(m) => Ok(Selection { video: m.clone(), audio: None }),
        None => bail!("YouTube offered no stream subfeed can play"),
    }
}

pub fn parse_info(json: &str) -> Result<VideoInfo> {
    let info: VideoInfo = serde_json::from_str(json).context("reading yt-dlp video JSON")?;
    match info.live_status.as_deref() {
        Some("is_upcoming") => bail!("This premiere hasn't started yet"),
        // Live streams come as HLS, which `select` doesn't use. A finished one ("post_live",
        // "was_live") has normal formats and plays.
        Some("is_live") => bail!("Live streams aren't supported yet"),
        _ => {}
    }
    Ok(info)
}

pub fn fetch_info(video_id: &str) -> Result<VideoInfo> {
    if !crate::thumbs::is_video_id(video_id) {
        bail!("not a video id: {video_id}");
    }
    let url = format!("https://www.youtube.com/watch?v={video_id}");
    parse_info(&ytdlp::run(&["--no-playlist", "-J", &url])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> VideoInfo {
        parse_info(include_str!("../tests/fixtures/ytdlp_info_C4Sq3Rot8FU.json")).unwrap()
    }

    #[test]
    fn parses_metadata() {
        let info = fixture();
        assert_eq!(info.title, "AI Labs Warn Race Makes Breach ‘Inevitable’");
        assert_eq!(info.channel.as_deref(), Some("AI Explained"));
        assert_eq!(info.chapters.len(), 4);
        assert_eq!(info.chapters[2], Chapter { start_time: 523.0, title: "Part 2: Why the Labs Keep Racing".into() });
        assert!(info.description.unwrap().contains("Chapters:"));
    }

    fn all() -> Vec<String> {
        vec!["av1".into(), "vp9".into(), "h264".into()]
    }

    #[test]
    fn battery_gets_720p_av1_and_original_opus() {
        let s = select(&fixture(), Policy::LOW_POWER, &all()).unwrap();
        assert_eq!(s.video.format_id, "398");
        assert_eq!(s.label(), "720p · AV1");
        assert_eq!(s.audio.as_ref().unwrap().format_id, "251");
        assert!(s.video.http_headers.contains_key("User-Agent"));
    }

    #[test]
    fn ac_gets_1080p_not_1440p() {
        let s = select(&fixture(), Policy::AC, &all()).unwrap();
        assert_eq!(s.video.format_id, "399");
    }

    #[test]
    fn follows_what_the_gpu_decodes() {
        // An older iGPU: no AV1 in hardware, so VP9.
        let s = select(&fixture(), Policy::AC, &["vp9".into(), "h264".into()]).unwrap();
        assert_eq!(s.label(), "1080p · VP9");
        // H.264 only: a hardware stream beats a higher-quality software one.
        let s = select(&fixture(), Policy::AC, &["h264".into()]).unwrap();
        assert_eq!(s.label(), "1080p · H.264");
        // Unknown (no VA-API, e.g. NVIDIA): VP9 first.
        let s = select(&fixture(), Policy::LOW_POWER, &[]).unwrap();
        assert_eq!(s.label(), "720p · VP9");
    }

    #[test]
    fn low_power_when_either_says_so() {
        assert_eq!(Policy::for_power(true, false), Policy::LOW_POWER);
        assert_eq!(Policy::for_power(false, true), Policy::LOW_POWER);
        assert_eq!(Policy::for_power(false, false), Policy::AC);
    }

    #[test]
    fn falls_back_to_muxed() {
        let mut info = fixture();
        info.formats.retain(|f| f.format_id == "18" || f.is_audio_only());
        info.formats.retain(|f| !f.is_audio_only());
        // The fixture may lack format 18; build one.
        info.formats.push(Format {
            format_id: "18".into(),
            url: Some("https://example.invalid/18".into()),
            protocol: "https".into(),
            vcodec: Some("avc1.42001E".into()),
            acodec: Some("mp4a.40.2".into()),
            width: Some(640),
            height: Some(360),
            fps: Some(30.0),
            tbr: None,
            dynamic_range: None,
            language_preference: None,
            http_headers: Default::default(),
        });
        let s = select(&info, Policy::LOW_POWER, &all()).unwrap();
        assert_eq!(s.video.format_id, "18");
        assert!(s.audio.is_none());
    }

    fn short() -> VideoInfo {
        parse_info(include_str!("../tests/fixtures/ytdlp_info_short_JVO00Ulr5BE.json")).unwrap()
    }

    #[test]
    fn captions_in_the_original_language() {
        let c = pick_captions(&short(), "en_US").unwrap();
        assert_eq!(c, Caption { url: "https://example.invalid/captions/en-orig.vtt".into(), label: "English (auto)".into() });
    }

    #[test]
    fn captions_translated_or_from_the_channel() {
        // A German desktop and an English video: YouTube's German translation.
        assert_eq!(pick_captions(&short(), "de_DE").unwrap().url, "https://example.invalid/captions/de.vtt");
        // No track for the language at all.
        assert_eq!(pick_captions(&short(), "ja_JP"), None);

        // The channel's own captions beat automatic ones, and SRT does when there's no WebVTT.
        let mut info = short();
        info.subtitles.insert(
            "en-GB".into(),
            vec![SubtitleFormat { ext: "srt".into(), url: "https://example.invalid/own.srt".into(), name: Some("English (UK)".into()) }],
        );
        let c = pick_captions(&info, "en_US").unwrap();
        assert_eq!((c.url.as_str(), c.label.as_str()), ("https://example.invalid/own.srt", "English (UK)"));

        // None anywhere.
        info.subtitles.clear();
        info.automatic_captions.clear();
        assert_eq!(pick_captions(&info, "en_US"), None);
    }

    #[test]
    fn live_streams_are_named() {
        let json = include_str!("../tests/fixtures/ytdlp_info_short_JVO00Ulr5BE.json")
            .replace("\"live_status\": \"not_live\"", "\"live_status\": \"is_live\"");
        assert!(json.contains("is_live"), "fixture has a live_status to swap");
        assert!(parse_info(&json).unwrap_err().to_string().contains("Live streams"));
    }

    #[test]
    fn vertical_short_counts_its_short_side() {
        let info = parse_info(include_str!("../tests/fixtures/ytdlp_info_short_JVO00Ulr5BE.json")).unwrap();
        let s = select(&info, Policy::LOW_POWER, &all()).unwrap();
        assert_eq!((s.video.format_id.as_str(), s.label()), ("398", "720p · AV1".to_string()), "720×1280, not 360×640");
        let s = select(&info, Policy::AC, &all()).unwrap();
        assert_eq!((s.video.format_id.as_str(), s.label()), ("399", "1080p · AV1".to_string()), "1080×1920, not 608×1080");
        assert_eq!(s.audio.unwrap().format_id, "251-20", "the original track, not a dub");
    }

    #[test]
    fn skips_hdr_and_hls() {
        let mut info = fixture();
        for f in &mut info.formats {
            if f.format_id == "398" {
                f.dynamic_range = Some("HDR10".into());
            }
        }
        let s = select(&info, Policy::LOW_POWER, &all()).unwrap();
        assert_eq!(s.video.format_id, "247", "720p VP9 when the AV1 one is HDR");
        assert_eq!(s.video.protocol, "https");
    }

    #[test]
    fn nothing_playable_is_an_error() {
        let mut info = fixture();
        info.formats.clear();
        assert!(select(&info, Policy::AC, &all()).is_err());
    }
}
