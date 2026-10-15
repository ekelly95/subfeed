//! Human-readable text for the feed cards.

/// `5:06`, `1:02:03`.
pub fn duration(seconds: i64) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// "3 hours ago", "yesterday", "2 weeks ago", then a date. Approximate dates (day precision,
/// from yt-dlp) never claim hours.
pub fn age(published: i64, approx: bool, now: i64) -> String {
    let secs = (now - published).max(0);
    let days = secs / 86_400;
    let plural = |n: i64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match days {
        0 if approx => "today".into(),
        0 if secs < 3600 => plural((secs / 60).max(1), "minute"),
        0 => plural(secs / 3600, "hour"),
        1 => "yesterday".into(),
        2..=13 => plural(days, "day"),
        14..=59 => plural(days / 7, "week"),
        60..=364 => plural(days / 30, "month"),
        _ => plural(days / 365, "year"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(duration(306), "5:06");
        assert_eq!(duration(59), "0:59");
        assert_eq!(duration(3723), "1:02:03");
    }

    #[test]
    fn ages() {
        let now = 10_000_000;
        assert_eq!(age(now - 30, false, now), "1 minute ago");
        assert_eq!(age(now - 3 * 3600, false, now), "3 hours ago");
        assert_eq!(age(now - 3 * 3600, true, now), "today");
        assert_eq!(age(now - 86_400 - 5, false, now), "yesterday");
        assert_eq!(age(now - 5 * 86_400, true, now), "5 days ago");
        assert_eq!(age(now - 20 * 86_400, false, now), "2 weeks ago");
        assert_eq!(age(now - 100 * 86_400, false, now), "3 months ago");
        assert_eq!(age(now - 400 * 86_400, false, now), "1 year ago");
        assert_eq!(age(now + 50, false, now), "1 minute ago", "clock skew");
    }
}
