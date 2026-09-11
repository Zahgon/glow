//! The two Lip Gloss styles used in glow's own chrome.

use crate::deps::lipgloss::{Color, Style};

/// Renders `s` in glow's keyword green.
pub fn keyword(s: &str) -> String {
    Style::new()
        .foreground(Color::parse("#04B575").expect("a literal colour parses"))
        .render(s)
}

/// Renders `s` as a 78-column paragraph with a two-space left pad.
pub fn paragraph(s: &str) -> String {
    Style::new().width(78).padding(0, 0, 0, 2).render(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_is_truecolor_green() {
        assert_eq!(
            keyword("with pizzazz"),
            "\u{1b}[38;2;4;181;117mwith pizzazz\u{1b}[m"
        );
    }

    #[test]
    fn paragraph_pads_to_78_columns() {
        let out = paragraph("hello");
        assert_eq!(out, format!("  hello{}", " ".repeat(71)));
    }
}
