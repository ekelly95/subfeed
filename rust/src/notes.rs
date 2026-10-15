//! "Save notes": a video as a Markdown note (Obsidian-flavoured) in a folder you choose: what it
//! is, its description, and its transcript in timestamped paragraphs under its chapters.
//!
//! The transcript is the channel's own captions in the video's language when there are any,
//! else YouTube's automatic captions of the original audio; a translation only as a last resort.
//! The note says which it is. An existing note is never overwritten (it may have your edits).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::stream::{SubtitleFormat, VideoInfo};
use crate::{format, thumbs};

/// The caption track a transcript comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptSource {
    pub url: String,
    /// YouTube's automatic captions rather than the channel's own.
    pub automatic: bool,
    /// Machine-translated from the video's language.
    pub translated: bool,
}

fn primary(code: &str) -> String {
    code.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase()
}

fn json3(formats: &[SubtitleFormat]) -> Option<String> {
    formats.iter().find(|f| f.ext == "json3").map(|f| f.url.clone())
}

/// Picks the track for a transcript: the channel's own in the video's language, the automatic
/// original ("<lang>-orig"), then the channel's own in `ui_lang`, then a translation to it.
pub fn transcript_source(info: &VideoInfo, ui_lang: &str) -> Option<TranscriptSource> {
    let spoken = info.language.as_deref().map(primary);
    let ui = primary(ui_lang);
    let manual_in = |lang: &str| {
        info.subtitles.iter().find(|(k, _)| primary(k) == lang).and_then(|(_, v)| json3(v))
    };
    let source = |url, automatic, translated| Some(TranscriptSource { url, automatic, translated });
    if let Some(url) = spoken.as_deref().and_then(manual_in) {
        return source(url, false, false);
    }
    let original = match &spoken {
        Some(s) => info.automatic_captions.get(&format!("{s}-orig")),
        None => info.automatic_captions.iter().find(|(k, _)| k.ends_with("-orig")).map(|(_, v)| v),
    };
    if let Some(url) = original.and_then(|v| json3(v)) {
        return source(url, true, false);
    }
    if let Some(url) = manual_in(&ui) {
        return source(url, false, spoken.is_some_and(|s| s != ui));
    }
    info.automatic_captions.get(&ui).and_then(|v| json3(v)).and_then(|url| source(url, true, true))
}

#[derive(Deserialize)]
struct Json3 {
    #[serde(default)]
    events: Vec<Event>,
}

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "tStartMs", default)]
    start_ms: i64,
    #[serde(default)]
    segs: Vec<Seg>,
}

#[derive(Deserialize)]
struct Seg {
    #[serde(default)]
    utf8: String,
}

/// (start seconds, text) per caption line, from YouTube's json3 format. Automatic captions come
/// word by word with blank "\n" events between lines; the channel's own come a line per event.
pub fn caption_lines(json: &str) -> Result<Vec<(f64, String)>> {
    let doc: Json3 = serde_json::from_str(json).context("reading captions")?;
    Ok(doc
        .events
        .into_iter()
        .filter_map(|e| {
            let text: String = e.segs.iter().map(|s| s.utf8.as_str()).collect();
            let text = drop_cues(&text.split_whitespace().collect::<Vec<_>>().join(" "));
            (!text.is_empty()).then(|| (e.start_ms as f64 / 1000.0, text))
        })
        .collect())
}

/// Drops what's there for viewers who can't hear it: short sound cues ("(air whooshing)",
/// "[Music]") and the "- " or ">>" that marks a new speaker.
fn drop_cues(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find(['(', '[']) {
        let close_char = if rest[open..].starts_with('(') { ')' } else { ']' };
        let Some(len) = rest[open..].find(close_char) else { break };
        let inner = &rest[open + 1..open + len];
        let cue = inner.split_whitespace().count() <= 6 && !inner.chars().any(|c| c.is_ascii_digit());
        out.push_str(&rest[..open]);
        if !cue {
            out.push_str(&rest[open..=open + len]);
        }
        rest = &rest[open + len + 1..];
    }
    out.push_str(rest);
    let text = out.split_whitespace().filter(|w| *w != ">>").collect::<Vec<_>>().join(" ");
    text.strip_prefix("- ").or_else(|| text.strip_prefix("-")).unwrap_or(&text).trim().replace(" - ", " ")
}

/// Groups caption lines into paragraphs: a new one after a sentence ends past 25 s, at 60 s
/// regardless, and at every chapter start. Returns (chapter index, start seconds, text).
pub fn paragraphs(lines: &[(f64, String)], chapter_starts: &[f64]) -> Vec<(usize, f64, String)> {
    let chapter_of = |t: f64| chapter_starts.iter().rposition(|&s| s <= t + 0.5).unwrap_or(0);
    let mut out: Vec<(usize, f64, String)> = Vec::new();
    for (t, text) in lines {
        let chapter = chapter_of(*t);
        let start_new = match out.last() {
            None => true,
            Some((c, start, para)) => {
                *c != chapter
                    || t - start >= 60.0
                    || (t - start >= 25.0 && para.ends_with(['.', '?', '!', '"', '”', ')']))
            }
        };
        if start_new {
            out.push((chapter, *t, text.clone()));
        } else if let Some((_, _, para)) = out.last_mut() {
            para.push(' ');
            para.push_str(text);
        }
    }
    out
}

/// Stops Obsidian reading "#tag" and "[[link]]" in someone else's text.
fn escape(text: &str) -> String {
    text.replace('#', "\\#").replace("[[", "\\[\\[")
}

fn yaml_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn watch_url(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

/// The note's Markdown. `transcript` is None when the video has no captions.
pub fn render(info: &VideoInfo, transcript: Option<(&TranscriptSource, &[(f64, String)])>, today: &str) -> String {
    let url = watch_url(&info.id);
    let published = info
        .timestamp
        .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
        .map(|t| t.with_timezone(&chrono::Local).date_naive());
    let duration = info.duration.map(|d| format::duration(d.round() as i64));
    let channel = info.channel.clone().unwrap_or_default();

    let mut md = String::from("---\n");
    md += &format!("title: {}\n", yaml_string(&info.title));
    md += &format!("channel: {}\n", yaml_string(&channel));
    md += &format!("url: {url}\n");
    if let Some(p) = published {
        md += &format!("published: {}\n", p.format("%Y-%m-%d"));
    }
    if let Some(d) = &duration {
        md += &format!("duration: {}\n", yaml_string(d));
    }
    md += &format!("saved: {today}\nsource: youtube\ntags: [youtube]\n---\n\n");
    md += &format!("# {}\n\n", escape(&info.title));
    let mut byline = vec![format!("**{}**", escape(&channel))];
    if let Some(p) = published {
        byline.push(p.format("%b %-d, %Y").to_string());
    }
    byline.extend(duration);
    byline.push(format!("[Watch on YouTube]({url})"));
    md += &byline.join(" · ");
    md += "\n\n";

    if let Some(desc) = info.description.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        md += "> [!abstract]- Description\n";
        for line in desc.lines() {
            md += &format!("> {}\n", escape(line).trim_end());
        }
        md += "\n";
    }

    md += "## Transcript\n\n";
    let Some((source, lines)) = transcript else {
        md += "*This video has no captions, so there's no transcript.*\n";
        return md;
    };
    md += match (source.automatic, source.translated) {
        (false, false) => "*From the channel's captions.*\n\n",
        (true, false) => "*From YouTube's automatic captions, so expect some mistakes.*\n\n",
        (false, true) => "*From the channel's captions, translated.*\n\n",
        (true, true) => "*Machine-translated from YouTube's automatic captions, so expect mistakes.*\n\n",
    };
    let starts: Vec<f64> = info.chapters.iter().map(|c| c.start_time).collect();
    let mut chapter = None;
    for (c, start, text) in paragraphs(lines, &starts) {
        if !info.chapters.is_empty() && chapter != Some(c) {
            md += &format!("### {}\n\n", escape(&info.chapters[c].title));
            chapter = Some(c);
        }
        let t = start.floor() as i64;
        md += &format!("[{}]({url}&t={t}s) {}\n\n", format::duration(t), escape(&text));
    }
    md
}

/// A file name from the title: no characters that break paths or Obsidian links.
pub fn file_name(info: &VideoInfo) -> String {
    let cleaned: String = info
        .title
        .chars()
        .map(|c| if "\\/:*?\"<>|#^[]".contains(c) || c.is_control() { ' ' } else { c })
        .collect();
    let mut name = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    name = name.trim_matches(['.', ' ']).to_string();
    if name.chars().count() > 100 {
        name = name.chars().take(100).collect::<String>().trim_end().to_string();
    }
    if name.is_empty() { info.id.clone() } else { name }
}

/// How to open a saved note: in Obsidian when it's inside a vault, else as a file.
pub fn open_url(path: &Path) -> String {
    let in_vault = path.ancestors().skip(1).any(|dir| dir.join(".obsidian").is_dir());
    let encoded: String = path
        .to_string_lossy()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    if in_vault { format!("obsidian://open?path={encoded}") } else { format!("file://{encoded}") }
}

pub struct Saved {
    pub path: PathBuf,
    /// A note for this video was already there, so nothing was written.
    pub existed: bool,
}

/// Fetches the transcript and writes the note into `dir`. A note that already exists is left
/// alone.
pub fn save(info: &VideoInfo, dir: &Path, ui_lang: &str) -> Result<Saved> {
    if !dir.is_dir() {
        bail!("{} isn't a folder", dir.display());
    }
    let path = dir.join(format!("{}.md", file_name(info)));
    if path.exists() {
        return Ok(Saved { path, existed: true });
    }
    let source = transcript_source(info, ui_lang);
    let lines = match &source {
        Some(s) => {
            let mut resp = thumbs::agent().get(&s.url).call().context("downloading captions")?;
            if resp.status() != 200 {
                bail!("captions download failed (HTTP {})", resp.status().as_u16());
            }
            caption_lines(&resp.body_mut().read_to_string()?)?
        }
        None => Vec::new(),
    };
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let md = render(info, source.as_ref().map(|s| (s, lines.as_slice())), &today);
    std::fs::write(&path, md).with_context(|| format!("writing {}", path.display()))?;
    Ok(Saved { path, existed: false })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::{Chapter, parse_info};

    fn short() -> VideoInfo {
        parse_info(include_str!("../tests/fixtures/ytdlp_info_short_JVO00Ulr5BE.json")).unwrap()
    }

    #[test]
    fn transcript_prefers_the_original_language() {
        let src = transcript_source(&short(), "de_DE").unwrap();
        assert_eq!((src.url.as_str(), src.automatic, src.translated), ("https://example.invalid/captions/en-orig.json3", true, false));

        let mut info = short();
        info.subtitles.insert(
            "en".into(),
            vec![SubtitleFormat { ext: "json3".into(), url: "https://example.invalid/own.json3".into(), name: None }],
        );
        assert_eq!(transcript_source(&info, "en_US").unwrap().url, "https://example.invalid/own.json3");

        info.subtitles.clear();
        info.automatic_captions.clear();
        assert_eq!(transcript_source(&info, "en_US"), None);
    }

    #[test]
    fn reads_both_caption_kinds() {
        let manual = caption_lines(include_str!("../tests/fixtures/captions_manual.json3")).unwrap();
        assert_eq!(manual[0], (0.0, "In 1887, a young physics teacher attached".to_string()));
        assert!(manual.iter().all(|(_, t)| !t.contains("(air whooshing)")));
        let auto = caption_lines(include_str!("../tests/fixtures/captions_auto.json3")).unwrap();
        assert!(auto[0].1.starts_with("In 1887, a young"), "{:?}", auto[0]);
        assert!(auto.iter().all(|(_, t)| !t.is_empty() && !t.contains('\n')));
    }

    #[test]
    fn sound_cues_go() {
        assert_eq!(drop_cues("(air whooshing) He fired. [Music]"), "He fired.");
        assert_eq!(drop_cues("- It's hot. - That's crazy."), "It's hot. That's crazy.");
        assert_eq!(drop_cues(">> So, A tier. >> >> Next."), "So, A tier. Next.");
        // Real parentheses stay: long ones, and ones with numbers.
        assert_eq!(drop_cues("glass (which, as we will see later, matters a lot) melts"), "glass (which, as we will see later, matters a lot) melts");
        assert_eq!(drop_cues("in (1887)"), "in (1887)");
    }

    #[test]
    fn paragraphs_follow_time_sentences_and_chapters() {
        let lines: Vec<(f64, String)> = (0..20).map(|i| (i as f64 * 5.0, format!("Line {i}."))).collect();
        let paras = paragraphs(&lines, &[0.0, 50.0]);
        // A break after 25 s at a sentence end, and at the chapter starting at 50 s.
        let starts: Vec<(usize, f64)> = paras.iter().map(|(c, s, _)| (*c, *s)).collect();
        assert_eq!(starts, [(0, 0.0), (0, 25.0), (1, 50.0), (1, 75.0)]);
        assert!(paras[0].2.starts_with("Line 0. Line 1."));
    }

    #[test]
    fn renders_a_note() {
        let mut info = short();
        info.title = "The \"Best\" & Worst #Peptides".into();
        info.description = Some("Ranked.\n#shorts and [[links]]".into());
        info.chapters = vec![
            Chapter { start_time: 0.0, title: "Intro".into() },
            Chapter { start_time: 30.0, title: "S tier".into() },
        ];
        let source = TranscriptSource { url: String::new(), automatic: true, translated: false };
        let lines = vec![(0.0, "Peptides are everywhere.".to_string()), (31.0, "Semaglutide.".to_string())];
        let md = render(&info, Some((&source, &lines)), "2026-10-15");

        assert!(md.starts_with("---\ntitle: \"The \\\"Best\\\" & Worst #Peptides\"\nchannel: \"Jeff Nippard\"\n"), "{md}");
        assert!(md.contains("tags: [youtube]\n---\n\n# The \"Best\" & Worst \\#Peptides\n"));
        assert!(md.contains("> [!abstract]- Description\n> Ranked.\n> \\#shorts and \\[\\[links]]\n"));
        assert!(md.contains("*From YouTube's automatic captions, so expect some mistakes.*"));
        assert!(md.contains("### Intro\n\n[0:00](https://www.youtube.com/watch?v=JVO00Ulr5BE&t=0s) Peptides are everywhere.\n\n### S tier\n\n[0:31](https://www.youtube.com/watch?v=JVO00Ulr5BE&t=31s) Semaglutide.\n"));
        assert!(render(&info, None, "2026-10-15").contains("no captions"));
    }

    #[test]
    fn safe_file_names_and_open_links() {
        let mut info = short();
        info.title = "What's #1? A/B: \"Tested\" [2026]".into();
        assert_eq!(file_name(&info), "What's 1 A B Tested 2026");
        info.title = "...".into();
        assert_eq!(file_name(&info), "JVO00Ulr5BE");

        let dir = std::env::temp_dir().join(format!("subfeed-notes-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("vault/.obsidian")).unwrap();
        std::fs::create_dir_all(dir.join("plain")).unwrap();
        assert!(open_url(&dir.join("vault/My note.md")).starts_with("obsidian://open?path=/"));
        assert!(open_url(&dir.join("vault/My note.md")).ends_with("vault/My%20note.md"));
        assert!(open_url(&dir.join("plain/x.md")).starts_with("file:///"));

        // An existing note is left alone.
        let mut info = short();
        info.title = "Kept".into();
        std::fs::write(dir.join("plain/Kept.md"), "my edits").unwrap();
        let saved = save(&info, &dir.join("plain"), "en_US").unwrap();
        assert!(saved.existed);
        assert_eq!(std::fs::read_to_string(saved.path).unwrap(), "my edits");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
