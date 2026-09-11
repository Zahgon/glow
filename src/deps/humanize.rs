//! Human-friendly durations and plurals.
//!
//! Reimplements the parts of `github.com/dustin/go-humanize` glow uses: the
//! relative-time table and `english.Plural`.

/// A relative time point at which the format switches.
///
/// `d` is the upper bound of the magnitude, `format` may carry a `%d` for the
/// quantity and a `%s` for the label, and `div_by` is what the difference is
/// divided by to produce that quantity.
pub struct RelTimeMagnitude {
    /// Upper bound, in nanoseconds.
    pub d: i128,
    /// Format string.
    pub format: &'static str,
    /// Divisor, in nanoseconds.
    pub div_by: i128,
}

const SECOND: i128 = 1_000_000_000;
const MINUTE: i128 = 60 * SECOND;
const HOUR: i128 = 60 * MINUTE;
/// A day, as go-humanize counts one.
pub const DAY: i128 = 24 * HOUR;
/// A week.
pub const WEEK: i128 = 7 * DAY;
/// A month, as go-humanize counts one.
pub const MONTH: i128 = 30 * DAY;
/// A year.
pub const YEAR: i128 = 12 * MONTH;
/// The point past which durations stop being counted in years.
pub const LONG_TIME: i128 = 37 * YEAR;

/// The magnitudes glow passes to [`custom_rel_time`], which are also
/// go-humanize's defaults.
pub const MAGNITUDES: &[RelTimeMagnitude] = &[
    RelTimeMagnitude {
        d: SECOND,
        format: "now",
        div_by: SECOND,
    },
    RelTimeMagnitude {
        d: 2 * SECOND,
        format: "1 second %s",
        div_by: 1,
    },
    RelTimeMagnitude {
        d: MINUTE,
        format: "%d seconds %s",
        div_by: SECOND,
    },
    RelTimeMagnitude {
        d: 2 * MINUTE,
        format: "1 minute %s",
        div_by: 1,
    },
    RelTimeMagnitude {
        d: HOUR,
        format: "%d minutes %s",
        div_by: MINUTE,
    },
    RelTimeMagnitude {
        d: 2 * HOUR,
        format: "1 hour %s",
        div_by: 1,
    },
    RelTimeMagnitude {
        d: DAY,
        format: "%d hours %s",
        div_by: HOUR,
    },
    RelTimeMagnitude {
        d: 2 * DAY,
        format: "1 day %s",
        div_by: 1,
    },
    RelTimeMagnitude {
        d: WEEK,
        format: "%d days %s",
        div_by: DAY,
    },
    RelTimeMagnitude {
        d: 2 * WEEK,
        format: "1 week %s",
        div_by: 1,
    },
    RelTimeMagnitude {
        d: MONTH,
        format: "%d weeks %s",
        div_by: WEEK,
    },
    RelTimeMagnitude {
        d: 2 * MONTH,
        format: "1 month %s",
        div_by: 1,
    },
    RelTimeMagnitude {
        d: YEAR,
        format: "%d months %s",
        div_by: MONTH,
    },
    RelTimeMagnitude {
        d: 18 * MONTH,
        format: "1 year %s",
        div_by: 1,
    },
    RelTimeMagnitude {
        d: 2 * YEAR,
        format: "2 years %s",
        div_by: 1,
    },
    RelTimeMagnitude {
        d: LONG_TIME,
        format: "%d years %s",
        div_by: YEAR,
    },
    RelTimeMagnitude {
        d: i64::MAX as i128,
        format: "a long while %s",
        div_by: 1,
    },
];

/// Formats the signed difference between two instants, in nanoseconds, using
/// `a_label` when `a` is the earlier one and `b_label` when it is not.
pub fn custom_rel_time(
    diff_ns: i128,
    a_label: &str,
    b_label: &str,
    magnitudes: &[RelTimeMagnitude],
) -> String {
    let (label, diff) = if diff_ns < 0 {
        (b_label, -diff_ns)
    } else {
        (a_label, diff_ns)
    };

    // The first magnitude whose bound is strictly greater than the difference.
    let n = magnitudes
        .iter()
        .position(|m| m.d > diff)
        .unwrap_or(magnitudes.len() - 1)
        .min(magnitudes.len() - 1);
    let mag = &magnitudes[n];

    mag.format
        .replace("%d", &(diff / mag.div_by).to_string())
        .replace("%s", label)
}

/// `english.Plural`: the quantity followed by the right form of the word.
pub fn plural(quantity: i64, singular: &str, plural_form: &str) -> String {
    format!(
        "{quantity} {}",
        plural_word(quantity, singular, plural_form)
    )
}

/// The irregular plurals go-humanize knows, because they are common technical
/// terms rather than because the rules below could reach them.
const SPECIAL_PLURALS: [(&str, &str); 3] = [
    ("index", "indices"),
    ("matrix", "matrices"),
    ("vertex", "vertices"),
];

/// `english.PluralWord`: the singular or the guessed plural.
pub fn plural_word(quantity: i64, singular: &str, plural_form: &str) -> String {
    if quantity == 1 {
        return singular.to_string();
    }
    if !plural_form.is_empty() {
        return plural_form.to_string();
    }
    if let Some((_, plural)) = SPECIAL_PLURALS.iter().find(|(s, _)| *s == singular) {
        return (*plural).to_string();
    }

    for ending in ["s", "sh", "tch", "x"] {
        if singular.ends_with(ending) {
            return format!("{singular}es");
        }
    }

    let bytes = singular.as_bytes();
    let l = bytes.len();
    if l >= 2 && bytes[l - 1] == b'o' && !is_vowel(bytes[l - 2]) {
        return format!("{singular}es");
    }
    if l >= 2 && bytes[l - 1] == b'y' && !is_vowel(bytes[l - 2]) {
        return format!("{}ies", &singular[..l - 1]);
    }
    format!("{singular}s")
}

fn is_vowel(b: u8) -> bool {
    matches!(
        b,
        b'A' | b'E' | b'I' | b'O' | b'U' | b'a' | b'e' | b'i' | b'o' | b'u'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(ns: i128) -> String {
        custom_rel_time(ns, "ago", "from now", MAGNITUDES)
    }

    #[test]
    fn each_magnitude_has_its_own_wording() {
        assert_eq!(rel(0), "now");
        assert_eq!(rel(SECOND), "1 second ago");
        assert_eq!(rel(30 * SECOND), "30 seconds ago");
        assert_eq!(rel(MINUTE), "1 minute ago");
        assert_eq!(rel(3 * MINUTE), "3 minutes ago");
        assert_eq!(rel(HOUR), "1 hour ago");
        assert_eq!(rel(3 * HOUR), "3 hours ago");
        assert_eq!(rel(DAY), "1 day ago");
        assert_eq!(rel(2 * DAY), "2 days ago");
        assert_eq!(rel(WEEK), "1 week ago");
        assert_eq!(rel(3 * WEEK), "3 weeks ago");
        assert_eq!(rel(MONTH), "1 month ago");
        assert_eq!(rel(3 * MONTH), "3 months ago");
        assert_eq!(rel(YEAR), "1 year ago");
        assert_eq!(rel(2 * YEAR), "2 years ago");
        assert_eq!(rel(3 * YEAR), "3 years ago");
        assert_eq!(rel(40 * YEAR), "a long while ago");
    }

    #[test]
    fn a_future_instant_uses_the_other_label() {
        assert_eq!(rel(-3 * HOUR), "3 hours from now");
    }

    #[test]
    fn plurals_follow_the_regular_rules() {
        assert_eq!(plural(1, "document", ""), "1 document");
        assert_eq!(plural(0, "document", ""), "0 documents");
        assert_eq!(plural(2, "document", ""), "2 documents");
        assert_eq!(plural_word(2, "box", ""), "boxes");
        assert_eq!(plural_word(2, "party", ""), "parties");
        assert_eq!(plural_word(2, "day", ""), "days");
        assert_eq!(plural_word(2, "potato", ""), "potatoes");
        for (singular, plural) in SPECIAL_PLURALS {
            assert_eq!(plural_word(2, singular, ""), plural);
        }
        assert_eq!(plural_word(2, "mouse", "mice"), "mice");
    }
}
