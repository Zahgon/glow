//! A markdown document in the file listing.

use std::time::SystemTime;

use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

use crate::deps::humanize::{self, MAGNITUDES, WEEK};

/// What the listing shows for a document touched less than a minute ago.
pub const JUST_NOW: &str = "just now";

/// One markdown document.
#[derive(Debug, Clone, Default)]
pub struct Markdown {
    /// Full path of the local file.
    pub local_path: String,
    /// The value filtering matches against.
    ///
    /// It is kept here so filtered positions survive an edit to the note while
    /// a filter is active; it is ephemeral and only meaningful during
    /// filtering.
    pub filter_value: String,
    /// The document's contents.
    pub body: String,
    /// The name shown in the listing.
    pub note: String,
    /// Last modification time.
    pub modtime: Option<SystemTime>,
}

impl Markdown {
    /// Builds the value this document is filtered against.
    pub fn build_filter_value(&mut self) {
        self.filter_value = normalize(&self.note);
    }

    /// The modification time, relative to now.
    pub fn relative_time(&self) -> String {
        match self.modtime {
            Some(t) => relative_time(t),
            None => relative_time(SystemTime::UNIX_EPOCH),
        }
    }
}

/// Normalises text for filtering by removing diacritics, so `ö` becomes `o`.
///
/// The transform is NFD, then drop the Unicode `Mn` (non-spacing mark)
/// category, then NFC.
pub fn normalize(input: &str) -> String {
    input
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .collect::<String>()
        .nfc()
        .collect()
}

/// Formats an instant relative to now.
pub fn relative_time(then: SystemTime) -> String {
    relative_time_from(then, SystemTime::now())
}

/// Formats `then` relative to `now`.
pub fn relative_time_from(then: SystemTime, now: SystemTime) -> String {
    let diff_ns = duration_between(now, then);
    if (0..60 * 1_000_000_000).contains(&diff_ns) {
        return JUST_NOW.to_string();
    }
    if diff_ns < WEEK {
        return humanize::custom_rel_time(diff_ns, "ago", "from now", MAGNITUDES);
    }
    absolute_time(then)
}

/// `time.Format("02 Jan 2006 15:04 MST")`.
pub fn absolute_time(then: SystemTime) -> String {
    let secs = match then.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    let local = chrono::DateTime::from_timestamp(secs, 0)
        .map(|utc| utc.with_timezone(&chrono::Local))
        .unwrap_or_else(chrono::Local::now);
    format!(
        "{} {}",
        local.format("%d %b %Y %H:%M"),
        zone_abbreviation(secs)
    )
}

/// The zone abbreviation `%Z` stands for, which chrono cannot supply for a
/// fixed-offset local time.
#[cfg(unix)]
fn zone_abbreviation(secs: i64) -> String {
    // SAFETY: `localtime_r` and `strftime` write into buffers we own.
    unsafe {
        let t = secs as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return String::new();
        }
        // `c_char` is signed on x86-64 and unsigned on aarch64, so the buffer
        // has to be spelled with the alias rather than with `i8`.
        let mut buf = [0 as libc::c_char; 32];
        let fmt = c"%Z";
        let n = libc::strftime(buf.as_mut_ptr(), buf.len(), fmt.as_ptr(), &tm);
        if n == 0 {
            return String::new();
        }
        let bytes: Vec<u8> = buf[..n].iter().map(|c| *c as u8).collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

/// The numeric offset, where no zone database is reachable.
#[cfg(not(unix))]
fn zone_abbreviation(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|utc| utc.with_timezone(&chrono::Local).format("%z").to_string())
        .unwrap_or_default()
}

/// `b.Sub(a)` in nanoseconds, positive when `a` is the earlier instant.
fn duration_between(b: SystemTime, a: SystemTime) -> i128 {
    match b.duration_since(a) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn diacritics_are_removed() {
        assert_eq!(normalize("ö"), "o");
        assert_eq!(normalize("Ünïcödé.md"), "Unicode.md");
        assert_eq!(normalize("plain.md"), "plain.md");
    }

    #[test]
    fn anything_under_a_minute_is_just_now() {
        let now = SystemTime::now();
        assert_eq!(relative_time_from(now, now), JUST_NOW);
        assert_eq!(
            relative_time_from(now - Duration::from_secs(59), now),
            JUST_NOW
        );
    }

    #[test]
    fn up_to_a_week_is_humanised() {
        let now = SystemTime::now();
        for (secs, want) in [
            (60u64, "1 minute ago"),
            (3 * 60, "3 minutes ago"),
            (3600, "1 hour ago"),
            (3 * 3600, "3 hours ago"),
            (86400, "1 day ago"),
            (3 * 86400, "3 days ago"),
        ] {
            assert_eq!(
                relative_time_from(now - Duration::from_secs(secs), now),
                want
            );
        }
    }

    #[test]
    fn beyond_a_week_is_an_absolute_date() {
        let now = SystemTime::now();
        let out = relative_time_from(now - Duration::from_secs(30 * 86400), now);
        assert!(!out.contains("ago"), "{out}");
        // "02 Jan 2006 15:04 MST"
        let parts: Vec<&str> = out.split(' ').collect();
        assert_eq!(parts.len(), 5, "{out}");
        assert_eq!(parts[0].len(), 2);
        assert_eq!(parts[3].len(), 5);
    }

    #[test]
    fn the_filter_value_is_the_normalised_note() {
        let mut md = Markdown {
            note: "Über.md".into(),
            ..Markdown::default()
        };
        md.build_filter_value();
        assert_eq!(md.filter_value, "Uber.md");
    }
}
