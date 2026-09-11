//! The single-line text input.

use crate::deps::ansi;
use crate::deps::lipgloss::Style;

/// Styling for the text input's parts.
#[derive(Debug, Clone, Default)]
pub struct Styles {
    /// The prompt shown before the value.
    pub prompt: Style,
    /// The value itself.
    pub text: Style,
    /// The virtual cursor.
    pub cursor: Style,
}

/// The text input model.
///
/// Glow uses it with a virtual cursor — one drawn into the string rather than
/// moved on the terminal — because the input is rendered inside a larger view.
#[derive(Debug, Clone, Default)]
pub struct Model {
    /// Text shown before the value.
    pub prompt: String,
    /// Styling.
    pub styles: Styles,
    value: Vec<char>,
    pos: usize,
    focus: bool,
    blinked: bool,
    width: usize,
    offset: usize,
    offset_right: usize,
}

impl Model {
    /// An empty, blurred input.
    pub fn new() -> Model {
        Model {
            prompt: "> ".to_string(),
            ..Model::default()
        }
    }

    /// The current value.
    pub fn value(&self) -> String {
        self.value.iter().collect()
    }

    /// Replaces the value.
    pub fn set_value(&mut self, s: &str) {
        self.value = sanitize(s);
        if self.pos > self.value.len() {
            self.pos = self.value.len();
        }
        self.handle_overflow();
    }

    /// The cursor position, in characters.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Moves the cursor, clamped to the value.
    pub fn set_cursor(&mut self, pos: usize) {
        self.pos = pos.min(self.value.len());
        self.handle_overflow();
    }

    /// Moves the cursor to the start.
    pub fn cursor_start(&mut self) {
        self.set_cursor(0);
    }

    /// Moves the cursor to the end.
    pub fn cursor_end(&mut self) {
        self.set_cursor(self.value.len());
    }

    /// Whether the input has focus.
    pub fn focused(&self) -> bool {
        self.focus
    }

    /// Gives the input focus.
    pub fn focus(&mut self) {
        self.focus = true;
        self.blinked = false;
    }

    /// Takes focus away.
    pub fn blur(&mut self) {
        self.focus = false;
    }

    /// Empties the input.
    pub fn reset(&mut self) {
        self.value.clear();
        self.set_cursor(0);
    }

    /// The width the value is windowed to.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Sets the width the value is windowed to.
    pub fn set_width(&mut self, w: i64) {
        self.width = w.max(0) as usize;
        self.handle_overflow();
    }

    /// Toggles the cursor between its drawn and hidden phases.
    pub fn blink(&mut self) {
        self.blinked = !self.blinked;
    }

    /// Handles a key press, returning whether the value changed.
    pub fn update_key(&mut self, key: &str, text: &str) {
        let old = self.value.clone();
        match key {
            "alt+backspace" | "ctrl+w" => self.delete_word_backward(),
            "backspace" | "ctrl+h" => {
                if !self.value.is_empty() && self.pos > 0 {
                    self.value.remove(self.pos - 1);
                    self.set_cursor(self.pos - 1);
                }
            }
            "alt+left" | "ctrl+left" | "alt+b" => self.word_backward(),
            "left" | "ctrl+b" => {
                if self.pos > 0 {
                    self.set_cursor(self.pos - 1);
                }
            }
            "alt+right" | "ctrl+right" | "alt+f" => self.word_forward(),
            "right" | "ctrl+f" => {
                if self.pos < self.value.len() {
                    self.set_cursor(self.pos + 1);
                }
            }
            "home" | "ctrl+a" => self.cursor_start(),
            "delete" | "ctrl+d" => {
                if self.pos < self.value.len() {
                    self.value.remove(self.pos);
                }
            }
            "end" | "ctrl+e" => self.cursor_end(),
            "ctrl+k" => self.value.truncate(self.pos),
            "ctrl+u" => {
                self.value.drain(..self.pos);
                self.set_cursor(0);
            }
            "alt+delete" | "alt+d" => self.delete_word_forward(),
            _ => self.insert(text),
        }
        if old != self.value {
            self.blinked = false;
        }
        self.handle_overflow();
    }

    fn insert(&mut self, text: &str) {
        for c in sanitize(text) {
            self.value.insert(self.pos, c);
            self.pos += 1;
        }
    }

    fn is_space(&self, i: usize) -> bool {
        // Go indexes the slice directly here and can run one past the end; the
        // out-of-range character is treated as a non-space, which is the branch
        // every non-panicking path takes.
        self.value.get(i).is_some_and(|c| c.is_whitespace())
    }

    fn word_backward(&mut self) {
        if self.pos == 0 || self.value.is_empty() {
            return;
        }
        let mut i = self.pos as i64 - 1;
        while i >= 0 && self.is_space(i as usize) {
            self.set_cursor(self.pos - 1);
            i -= 1;
        }
        while i >= 0 && !self.is_space(i as usize) {
            self.set_cursor(self.pos - 1);
            i -= 1;
        }
    }

    fn word_forward(&mut self) {
        if self.pos >= self.value.len() || self.value.is_empty() {
            return;
        }
        let mut i = self.pos;
        while i < self.value.len() && self.is_space(i) {
            self.set_cursor(self.pos + 1);
            i += 1;
        }
        while i < self.value.len() && !self.is_space(i) {
            self.set_cursor(self.pos + 1);
            i += 1;
        }
    }

    fn delete_word_backward(&mut self) {
        if self.pos == 0 || self.value.is_empty() {
            return;
        }
        let old_pos = self.pos;

        self.set_cursor(self.pos - 1);
        while self.is_space(self.pos) {
            if self.pos == 0 {
                break;
            }
            self.set_cursor(self.pos - 1);
        }
        while self.pos > 0 {
            if !self.is_space(self.pos) {
                self.set_cursor(self.pos - 1);
            } else {
                // Keep the space that ends the previous word.
                self.set_cursor(self.pos + 1);
                break;
            }
        }

        let pos = self.pos;
        if old_pos > self.value.len() {
            self.value.truncate(pos);
        } else {
            self.value.drain(pos..old_pos);
        }
    }

    fn delete_word_forward(&mut self) {
        if self.pos >= self.value.len() || self.value.is_empty() {
            return;
        }
        let old_pos = self.pos;
        self.set_cursor(self.pos + 1);
        while self.is_space(self.pos) {
            self.set_cursor(self.pos + 1);
            if self.pos >= self.value.len() {
                break;
            }
        }
        while self.pos < self.value.len() {
            if !self.is_space(self.pos) {
                self.set_cursor(self.pos + 1);
            } else {
                break;
            }
        }

        let pos = self.pos;
        self.value.drain(old_pos..pos);
        self.set_cursor(old_pos);
    }

    /// Keeps the visible window of the value around the cursor.
    fn handle_overflow(&mut self) {
        let value_width = ansi::string_width(&self.value());
        if self.width == 0 || value_width <= self.width {
            self.offset = 0;
            self.offset_right = self.value.len();
            return;
        }
        self.offset_right = self.offset_right.min(self.value.len());

        if self.pos < self.offset {
            self.offset = self.pos;
            let mut w = 0usize;
            let mut i = 0usize;
            let runes = &self.value[self.offset..];
            while i < runes.len() && w <= self.width {
                w += char_width(runes[i]);
                if w <= self.width + 1 {
                    i += 1;
                }
            }
            self.offset_right = self.offset + i;
        } else if self.pos >= self.offset_right {
            self.offset_right = self.pos;
            let mut w = 0usize;
            let runes = &self.value[..self.offset_right];
            let mut i = runes.len() as i64 - 1;
            while i > 0 && w < self.width {
                w += char_width(runes[i as usize]);
                if w <= self.width {
                    i -= 1;
                }
            }
            let consumed = runes.len() as i64 - 1 - i;
            self.offset = (self.offset_right as i64 - consumed).max(0) as usize;
        }
    }

    /// Renders the prompt, the windowed value and the cursor.
    pub fn view(&self) -> String {
        let value =
            &self.value[self.offset.min(self.value.len())..self.offset_right.min(self.value.len())];
        let pos = self.pos.saturating_sub(self.offset);

        let mut v = self
            .styles
            .text
            .render(&value[..pos.min(value.len())].iter().collect::<String>());
        if pos < value.len() {
            v.push_str(&self.cursor_view(&value[pos].to_string()));
            v.push_str(
                &self
                    .styles
                    .text
                    .render(&value[pos + 1..].iter().collect::<String>()),
            );
        } else {
            v.push_str(&self.cursor_view(" "));
        }

        let val_width = ansi::string_width(&value.iter().collect::<String>());
        if self.width > 0 && val_width <= self.width {
            let mut padding = self.width - val_width;
            if val_width + padding <= self.width && pos < value.len() {
                padding += 1;
            }
            v.push_str(&self.styles.text.render(&" ".repeat(padding)));
        }

        format!("{}{v}", self.styles.prompt.render(&self.prompt))
    }

    fn cursor_view(&self, char: &str) -> String {
        if !self.focus || self.blinked {
            return self.styles.text.render(char);
        }
        self.styles.cursor.clone().reverse(true).render(char)
    }
}

/// Collapses the characters a single-line input cannot hold.
fn sanitize(s: &str) -> Vec<char> {
    s.chars()
        .map(|c| {
            if c == '\t' || c == '\n' || c == '\r' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

fn char_width(c: char) -> usize {
    ansi::string_width(&c.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(value: &str) -> Model {
        let mut m = Model::new();
        m.prompt = "Find:".into();
        m.focus();
        m.set_value(value);
        m.cursor_end();
        m
    }

    #[test]
    fn typing_inserts_at_the_cursor() {
        let mut m = input("");
        for c in ["a", "b", "c"] {
            m.update_key(c, c);
        }
        assert_eq!(m.value(), "abc");
        m.update_key("left", "");
        m.update_key("x", "x");
        assert_eq!(m.value(), "abxc");
    }

    #[test]
    fn backspace_and_delete_remove_around_the_cursor() {
        let mut m = input("abc");
        m.update_key("backspace", "");
        assert_eq!(m.value(), "ab");
        m.cursor_start();
        m.update_key("delete", "");
        assert_eq!(m.value(), "b");
        m.update_key("backspace", "");
        assert_eq!(m.value(), "b", "backspace at the start does nothing");
    }

    #[test]
    fn line_editing_keys_cut_around_the_cursor() {
        let mut m = input("hello world");
        m.set_cursor(5);
        m.update_key("ctrl+k", "");
        assert_eq!(m.value(), "hello");
        m.update_key("ctrl+u", "");
        assert_eq!(m.value(), "");
        assert_eq!(m.position(), 0);
    }

    #[test]
    fn word_keys_move_and_delete_by_word() {
        let mut m = input("one two three");
        m.update_key("alt+b", "");
        assert_eq!(m.position(), 8);
        m.update_key("ctrl+w", "");
        assert_eq!(m.value(), "one three", "the space before the word is kept");
        m.update_key("alt+b", "");
        assert_eq!(m.position(), 0);
        m.update_key("alt+d", "");
        assert_eq!(m.value(), " three");
    }

    #[test]
    fn reset_clears_the_value_and_the_cursor() {
        let mut m = input("abc");
        m.reset();
        assert_eq!(m.value(), "");
        assert_eq!(m.position(), 0);
    }

    #[test]
    fn the_view_shows_the_prompt_and_a_cursor_block() {
        let mut m = input("ab");
        m.set_width(4);
        let view = m.view();
        assert!(view.starts_with("Find:"), "{view}");
        assert!(view.contains("ab"), "{view}");
    }

    #[test]
    fn tabs_and_newlines_become_spaces() {
        let mut m = input("");
        m.update_key("x", "a\tb\nc");
        assert_eq!(m.value(), "a b c");
    }
}
