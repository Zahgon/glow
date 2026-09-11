//! ANSI-aware text measurement, wrapping and truncation.
//!
//! Reproduces the observable behaviour of `github.com/charmbracelet/x/ansi`
//! (`StringWidth`, `Wrap`) and `github.com/muesli/reflow` (`ansi.PrintableRuneWidth`,
//! `truncate.StringWithTail`), which the Go original relied on for every piece of
//! terminal layout it performed.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// A non-breaking space never acts as a wrap point.
const NBSP: char = '\u{00A0}';

/// Splits a string into (escape-sequence, printable-grapheme) tokens while
/// tracking where ANSI/OSC sequences begin and end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token<'a> {
    /// A printable grapheme cluster and its display width.
    Text(&'a str, usize),
    /// A control or escape sequence that occupies no columns.
    Escape(&'a str),
}

/// Tokenises `s` into printable graphemes and zero-width escape sequences.
pub fn tokenize(s: &str) -> Vec<Token<'_>> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            let end = escape_end(bytes, i);
            out.push(Token::Escape(&s[i..end]));
            i = end;
            continue;
        }
        // Take one grapheme cluster starting at i.
        let rest = &s[i..];
        let g = rest.graphemes(true).next().unwrap_or("");
        if g.is_empty() {
            break;
        }
        let w = if g.chars().all(|c| c.is_control()) {
            0
        } else {
            UnicodeWidthStr::width(g)
        };
        out.push(Token::Text(&s[i..i + g.len()], w));
        i += g.len();
    }
    out
}

/// Returns the byte index just past the escape sequence starting at `start`.
fn escape_end(b: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    if i >= b.len() {
        return b.len();
    }
    match b[i] {
        // CSI: ESC [ ... final byte in 0x40..=0x7E
        b'[' => {
            i += 1;
            while i < b.len() && !(0x40..=0x7e).contains(&b[i]) {
                i += 1;
            }
            (i + 1).min(b.len())
        }
        // OSC / DCS / SOS / PM / APC: terminated by BEL or ST (ESC \)
        b']' | b'P' | b'X' | b'^' | b'_' => {
            i += 1;
            while i < b.len() {
                if b[i] == 0x07 {
                    return i + 1;
                }
                if b[i] == 0x1b && i + 1 < b.len() && b[i + 1] == b'\\' {
                    return i + 2;
                }
                i += 1;
            }
            b.len()
        }
        // Two-byte escape.
        _ => (i + 1).min(b.len()),
    }
}

/// Display width of `s`, ignoring ANSI escape sequences.
///
/// Equivalent to `ansi.StringWidth` and `reflow/ansi.PrintableRuneWidth`.
pub fn string_width(s: &str) -> usize {
    tokenize(s)
        .into_iter()
        .map(|t| match t {
            Token::Text(_, w) => w,
            Token::Escape(_) => 0,
        })
        .sum()
}

/// Word-wraps `s` to `limit` columns, hard-wrapping words that do not fit.
///
/// This is a direct port of `ansi.Wrap` from `charmbracelet/x/ansi`: `-` is
/// always a breakpoint, `breakpoints` adds more, ANSI sequences are preserved
/// and never counted, and a word longer than `limit` is broken mid-word.
// The accumulators below are reset inside macros that expand at several
// points; the lint only sees the expansion where a reset is not read again.
#[allow(unused_assignments)]
pub fn wrap(s: &str, limit: i64, breakpoints: &str) -> String {
    if limit < 1 {
        return s.to_string();
    }
    let limit = limit as usize;

    let mut buf = String::new();
    let mut word = String::new();
    let mut space = String::new();
    let mut space_width = 0usize;
    let mut cur_width = 0usize;
    let mut word_len = 0usize;

    macro_rules! add_space {
        () => {
            if space_width != 0 || !space.is_empty() {
                cur_width += space_width;
                buf.push_str(&space);
                space.clear();
                space_width = 0;
            }
        };
    }
    macro_rules! add_word {
        () => {
            if !word.is_empty() {
                add_space!();
                cur_width += word_len;
                buf.push_str(&word);
                word.clear();
                word_len = 0;
            }
        };
    }
    macro_rules! add_newline {
        () => {
            buf.push('\n');
            cur_width = 0;
            space.clear();
            space_width = 0;
        };
    }

    for token in tokenize(s) {
        match token {
            Token::Escape(e) => word.push_str(e),
            Token::Text(t, width) => {
                let c = t.chars().next().unwrap_or('\0');
                // Go's `wrap` runs two branches: one for multi-byte grapheme
                // clusters and one for single-byte input. They order their
                // checks differently, and the difference is visible whenever a
                // word is exactly as wide as the limit.
                let multibyte = t.len() > 1;
                if c == '\n' {
                    if word_len == 0 {
                        if cur_width + space_width > limit {
                            cur_width = 0;
                        } else {
                            buf.push_str(&space);
                        }
                        space.clear();
                        space_width = 0;
                    }
                    add_word!();
                    add_newline!();
                } else if c.is_whitespace() && c != NBSP {
                    add_word!();
                    space.push_str(t);
                    space_width += if multibyte { width } else { 1 };
                } else if c == '-' || breakpoints.contains(c) {
                    add_space!();
                    let keep_in_word = if multibyte {
                        cur_width + word_len + width > limit
                    } else {
                        cur_width + word_len >= limit
                    };
                    if keep_in_word {
                        word.push_str(t);
                        word_len += width;
                    } else {
                        add_word!();
                        buf.push_str(t);
                        cur_width += width;
                    }
                } else if multibyte {
                    if word_len + width > limit {
                        add_word!();
                    }
                    word.push_str(t);
                    word_len += width;
                    if cur_width + word_len + space_width > limit {
                        add_newline!();
                    }
                    if word_len == limit {
                        add_word!();
                    }
                } else {
                    if cur_width == limit {
                        add_newline!();
                    }
                    word.push_str(t);
                    word_len += width;
                    if word_len == limit {
                        add_word!();
                    }
                    if cur_width + word_len + space_width > limit {
                        add_newline!();
                    }
                }
            }
        }
    }

    if word_len == 0 {
        if cur_width + space_width > limit {
            // The trailing whitespace does not fit; drop it.
        } else {
            buf.push_str(&space);
        }
        space.clear();
    }
    add_word!();
    let _ = (cur_width, space_width, space);
    buf
}

/// Truncates `s` to `width` columns, appending `tail` when truncation happens.
///
/// Equivalent to `reflow/truncate.StringWithTail`: the tail's own width is
/// subtracted from the budget, ANSI sequences are preserved, and a string that
/// already fits is returned unchanged.
pub fn truncate_with_tail(s: &str, width: usize, tail: &str) -> String {
    if string_width(s) <= width {
        return s.to_string();
    }
    let tail_width = string_width(tail);
    if width < tail_width {
        return String::new();
    }
    let budget = width - tail_width;

    let mut out = String::new();
    let mut cur = 0usize;
    let mut in_style = false;
    for token in tokenize(s) {
        match token {
            Token::Escape(e) => {
                out.push_str(e);
                if e.starts_with("\u{1b}[") {
                    in_style = !(e == "\u{1b}[0m" || e == "\u{1b}[m");
                }
            }
            Token::Text(t, w) => {
                if cur + w > budget {
                    break;
                }
                cur += w;
                out.push_str(t);
            }
        }
    }
    out.push_str(tail);
    if in_style {
        out.push_str("\u{1b}[0m");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_width_ignores_escapes() {
        assert_eq!(string_width("abc"), 3);
        assert_eq!(string_width("\u{1b}[38;5;252mabc\u{1b}[m"), 3);
        assert_eq!(string_width("日本"), 4);
        assert_eq!(string_width(""), 0);
    }

    #[test]
    fn wrap_breaks_on_spaces() {
        assert_eq!(wrap("hello world", 5, ""), "hello\nworld");
        assert_eq!(wrap("hello world", 11, ""), "hello world");
    }

    #[test]
    fn wrap_hard_breaks_long_words() {
        assert_eq!(wrap("abcdefghij", 4, ""), "abcd\nefgh\nij");
    }

    #[test]
    fn wrap_zero_limit_is_identity() {
        assert_eq!(wrap("hello world", 0, ""), "hello world");
    }

    #[test]
    fn wrap_preserves_newlines() {
        assert_eq!(wrap("a\nb", 10, ""), "a\nb");
    }

    #[test]
    fn truncate_appends_tail() {
        assert_eq!(truncate_with_tail("hello world", 8, "…"), "hello w…");
        assert_eq!(truncate_with_tail("short", 8, "…"), "short");
    }
}
