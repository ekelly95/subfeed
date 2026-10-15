//! Plain data types shared by the importers, the feed sources and the database.

/// A channel the user follows. `id` is the stable `UC…` id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub id: String,
    pub title: String,
}

impl Channel {
    pub fn url(&self) -> String {
        channel_url(&self.id)
    }
}

pub fn channel_url(id: &str) -> String {
    format!("https://www.youtube.com/channel/{id}")
}

/// `UC` followed by 22 base64url characters.
pub fn is_channel_id(s: &str) -> bool {
    s.len() == 24
        && s.starts_with("UC")
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// One video as reported by a feed source, before it is merged into the database.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoEntry {
    pub id: String,
    pub channel_id: String,
    pub title: String,
    /// Unix seconds.
    pub published_at: i64,
    /// True when the date comes from YouTube's "3 days ago" text rather than an exact timestamp.
    pub published_approx: bool,
    pub duration_s: Option<i64>,
    pub thumb_url: Option<String>,
    pub is_short: bool,
}

/// A stable thumbnail URL. Thumbnails listed on channel pages are signed and expire.
pub fn thumb_url(video_id: &str) -> String {
    format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_ids() {
        assert!(is_channel_id("UCsBjURrPoezykLs9EqgamOA"));
        assert!(!is_channel_id("UUsBjURrPoezykLs9EqgamOA"));
        assert!(!is_channel_id("UCsBjURrPoezykLs9Eqgam"));
        assert!(!is_channel_id("@Fireship"));
    }
}
