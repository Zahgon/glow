//! Roff document generation.
//!
//! Reimplements `github.com/muesli/roff`: the macro constants, the
//! "never emit a blank line" write rule, and the text escaping.

/// Title heading (document structure macro).
const TITLE_HEADING: &str = ".TH";
/// Paragraph macro.
const PARAGRAPH: &str = "\n.PP";
/// Relative-indent start (document structure macro).
const INDENT: &str = "\n.RS";
/// Relative-indent end (document structure macro).
const INDENT_END: &str = "\n.RE";
/// Indented paragraph.
const INDENTED_PARAGRAPH: &str = "\n.IP";
/// Section heading (document structure macro).
const SECTION_HEADING: &str = "\n.SH";
/// Tagged paragraph.
const TAGGED_PARAGRAPH: &str = "\n.TP";

/// Bold escape.
pub const BOLD: &str = r"\fB";
/// Italic escape.
pub const ITALIC: &str = r"\fI";
/// Return to the previous font setting.
pub const PREVIOUS_FONT: &str = r"\fP";

/// A roff document under construction.
#[derive(Debug, Default)]
pub struct Document {
    buffer: String,
}

impl Document {
    /// An empty document.
    pub fn new() -> Document {
        Document::default()
    }

    /// Appends `s`, dropping its leading newline if the buffer already ends in
    /// one — roff renderers can break on blank lines.
    fn write(&mut self, s: &str) {
        let s = if self.buffer.ends_with('\n') {
            s.strip_prefix('\n').unwrap_or(s)
        } else {
            s
        };
        self.buffer.push_str(s);
    }

    fn writeln(&mut self, s: &str) {
        let owned = format!("{s}\n");
        self.write(&owned);
    }

    /// Writes the title heading of the document.
    pub fn heading(&mut self, section: u32, title: &str, description: &str, date: &str) {
        let line = format!(
            "{TITLE_HEADING} {} {section} \"{date}\" \"{title}\" \"{description}\"",
            title.to_uppercase()
        );
        self.write(&line);
    }

    /// Starts a new paragraph.
    pub fn paragraph(&mut self) {
        self.writeln(PARAGRAPH);
    }

    /// Increases the indentation level.
    pub fn indent(&mut self, n: i32) {
        if n >= 0 {
            self.writeln(&format!("{INDENT} {n}"));
        } else {
            self.writeln(INDENT);
        }
    }

    /// Decreases the indentation level.
    pub fn indent_end(&mut self) {
        self.writeln(INDENT_END);
    }

    /// Starts a new tagged paragraph.
    pub fn tagged_paragraph(&mut self, indentation: i32) {
        if indentation >= 0 {
            self.writeln(&format!("{TAGGED_PARAGRAPH} {indentation}"));
        } else {
            self.writeln(TAGGED_PARAGRAPH);
        }
    }

    /// Writes a list item.
    pub fn list(&mut self, text: &str) {
        self.writeln(&format!(
            "{INDENTED_PARAGRAPH} \\(bu 3\n{}",
            escape_text(text.trim())
        ));
    }

    /// Writes a section heading.
    pub fn section(&mut self, text: &str) {
        self.writeln(&format!("{SECTION_HEADING} {}", text.to_uppercase()));
    }

    /// Ends the current section.
    pub fn end_section(&mut self) {
        self.writeln("");
    }

    /// Writes text, turning `\n`-separated lines into paragraphs and
    /// `*`-prefixed lines into a bulleted list.
    pub fn text(&mut self, text: &str) {
        let mut in_list = false;
        for (i, s) in text.split('\n').enumerate() {
            if i > 0 && !in_list {
                self.paragraph();
            }

            if let Some(item) = s.strip_prefix('*') {
                if !in_list {
                    self.indent(-1);
                    in_list = true;
                }
                self.list(item);
            } else {
                if in_list {
                    self.indent_end();
                    in_list = false;
                }
                let escaped = escape_text(s);
                self.write(&escaped);
            }
        }
    }

    /// Writes text in bold.
    pub fn text_bold(&mut self, text: &str) {
        self.write(BOLD);
        self.text(text);
        self.write(PREVIOUS_FONT);
    }

    /// Writes text in italic.
    pub fn text_italic(&mut self, text: &str) {
        self.write(ITALIC);
        self.text(text);
        self.write(PREVIOUS_FONT);
    }
}

impl std::fmt::Display for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.buffer)
    }
}

/// Escapes roff's two significant characters in running text.
fn escape_text(s: &str) -> String {
    s.replace('\\', r"\e").replace('.', r"\&.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_heading_orders_date_before_title() {
        let mut d = Document::new();
        d.heading(1, "Title", "A short description", "2022-01-11");
        assert_eq!(
            d.to_string(),
            ".TH TITLE 1 \"2022-01-11\" \"Title\" \"A short description\""
        );
    }

    #[test]
    fn section_heading_is_upper_cased() {
        let mut d = Document::new();
        d.section("Test");
        assert_eq!(d.to_string(), "\n.SH TEST\n");
    }

    #[test]
    fn bold_text_is_wrapped_in_font_escapes() {
        let mut d = Document::new();
        d.text_bold("Test");
        assert_eq!(d.to_string(), "\\fBTest\\fP");
    }

    #[test]
    fn periods_and_backslashes_are_escaped() {
        let mut d = Document::new();
        d.text("options...");
        assert_eq!(d.to_string(), "options\\&.\\&.\\&.");
        assert!(
            !escape_text("TUI-mode only").contains('\\'),
            "hyphens are literal"
        );
    }

    #[test]
    fn a_leading_newline_never_makes_a_blank_line() {
        let mut d = Document::new();
        d.section("One");
        d.section("Two");
        assert_eq!(d.to_string(), "\n.SH ONE\n.SH TWO\n");
    }

    #[test]
    fn end_section_after_a_newline_writes_nothing() {
        let mut d = Document::new();
        d.section("One");
        d.end_section();
        assert_eq!(d.to_string(), "\n.SH ONE\n");
        d.text_bold("x");
        d.end_section();
        assert_eq!(d.to_string(), "\n.SH ONE\n\\fBx\\fP\n");
    }

    #[test]
    fn multi_line_text_becomes_paragraphs() {
        let mut d = Document::new();
        d.text("one\ntwo");
        assert_eq!(d.to_string(), "one\n.PP\ntwo");
    }

    #[test]
    fn star_prefixed_lines_become_a_list() {
        let mut d = Document::new();
        d.text("*one\n*two\nafter");
        assert_eq!(
            d.to_string(),
            "\n.RS\n.IP \\(bu 3\none\n.IP \\(bu 3\ntwo\n.RE\nafter"
        );
    }
}
