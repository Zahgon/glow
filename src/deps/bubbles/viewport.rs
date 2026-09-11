//! The scrolling viewport.

use crate::deps::ansi;
use crate::deps::lipgloss::Style;

/// The viewport model.
///
/// Glow builds it with `viewport.New()`, so soft wrapping, height filling and
/// the gutter are all off, and the style is empty.
#[derive(Debug, Clone, Default)]
pub struct Model {
    width: usize,
    height: usize,
    y_offset: usize,
    x_offset: usize,
    horizontal_step: usize,
    lines: Vec<String>,
    longest_line_width: usize,
}

impl Model {
    /// A viewport with the default key handling.
    pub fn new() -> Model {
        Model {
            horizontal_step: 6,
            ..Model::default()
        }
    }

    /// The viewport's height.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Sets the viewport's height.
    pub fn set_height(&mut self, h: usize) {
        self.height = h;
    }

    /// The viewport's width.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Sets the viewport's width.
    pub fn set_width(&mut self, w: usize) {
        self.width = w;
    }

    /// Whether the viewport is at the very top.
    pub fn at_top(&self) -> bool {
        self.y_offset == 0
    }

    /// Whether the viewport is at or past the bottom.
    pub fn at_bottom(&self) -> bool {
        self.y_offset >= self.max_y_offset()
    }

    /// Whether the viewport is scrolled past the last line.
    pub fn past_bottom(&self) -> bool {
        self.y_offset > self.max_y_offset()
    }

    /// How far down the content the viewport is, between 0 and 1.
    pub fn scroll_percent(&self) -> f64 {
        let total = self.lines.len();
        if self.height >= total {
            return 1.0;
        }
        let v = self.y_offset as f64 / (total as f64 - self.height as f64);
        v.clamp(0.0, 1.0)
    }

    /// Replaces the content, splitting it into lines.
    pub fn set_content(&mut self, s: &str) {
        let mut lines: Vec<String> = s.split('\n').map(str::to_string).collect();
        // A single blank line is no content at all.
        if lines.len() == 1 && ansi::string_width(&lines[0]) == 0 {
            lines.clear();
        } else {
            // Any embedded line ending becomes a line of its own.
            let mut expanded: Vec<String> = Vec::with_capacity(lines.len());
            for line in lines.drain(..) {
                if line.contains('\r') || line.contains('\n') {
                    let normalised = line.replace("\r\n", "\n");
                    expanded.extend(normalised.split('\n').map(str::to_string));
                } else {
                    expanded.push(line);
                }
            }
            lines = expanded;
        }
        self.longest_line_width = lines
            .iter()
            .map(|l| ansi::string_width(l))
            .max()
            .unwrap_or(0);
        self.lines = lines;
        if self.y_offset > self.max_y_offset() {
            self.goto_bottom();
        }
    }

    /// The total number of lines, visible or not.
    pub fn total_line_count(&self) -> usize {
        self.lines.len()
    }

    fn max_y_offset(&self) -> usize {
        self.lines.len().saturating_sub(self.height)
    }

    fn max_x_offset(&self) -> usize {
        self.longest_line_width.saturating_sub(self.width)
    }

    /// The current vertical scroll position.
    pub fn y_offset(&self) -> usize {
        self.y_offset
    }

    /// Sets the vertical scroll position, clamped to the content.
    pub fn set_y_offset(&mut self, n: usize) {
        self.y_offset = n.min(self.max_y_offset());
    }

    fn set_x_offset(&mut self, n: i64) {
        self.x_offset = n.clamp(0, self.max_x_offset() as i64) as usize;
    }

    /// Scrolls to the top.
    pub fn goto_top(&mut self) {
        if self.at_top() {
            return;
        }
        self.set_y_offset(0);
    }

    /// Scrolls to the bottom.
    pub fn goto_bottom(&mut self) {
        self.set_y_offset(self.max_y_offset());
    }

    /// Scrolls down by a whole viewport.
    pub fn page_down(&mut self) {
        if self.at_bottom() {
            return;
        }
        self.scroll_down(self.height);
    }

    /// Scrolls up by a whole viewport.
    pub fn page_up(&mut self) {
        if self.at_top() {
            return;
        }
        self.scroll_up(self.height);
    }

    /// Scrolls down by half a viewport.
    pub fn half_page_down(&mut self) {
        if self.at_bottom() {
            return;
        }
        self.scroll_down(self.height / 2);
    }

    /// Scrolls up by half a viewport.
    pub fn half_page_up(&mut self) {
        if self.at_top() {
            return;
        }
        self.scroll_up(self.height / 2);
    }

    /// Scrolls down `n` lines.
    pub fn scroll_down(&mut self, n: usize) {
        if self.at_bottom() || n == 0 || self.lines.is_empty() {
            return;
        }
        self.set_y_offset(self.y_offset + n);
    }

    /// Scrolls up `n` lines.
    pub fn scroll_up(&mut self, n: usize) {
        if self.at_top() || n == 0 || self.lines.is_empty() {
            return;
        }
        self.set_y_offset(self.y_offset.saturating_sub(n));
    }

    /// The lines that are currently on screen.
    pub fn visible_lines(&self) -> Vec<String> {
        if self.height == 0 || self.width == 0 {
            return Vec::new();
        }
        let ridx = self.y_offset.min(self.lines.len());
        let bottom = (ridx + self.height).clamp(ridx, self.lines.len());
        let mut lines: Vec<String> = self.lines[ridx..bottom].to_vec();

        if self.x_offset == 0 && self.longest_line_width <= self.width {
            return lines;
        }
        for line in &mut lines {
            *line = ansi::truncate_with_tail(line, self.x_offset + self.width, "");
        }
        lines
    }

    /// Handles a key press, using the viewport's default bindings.
    pub fn update_key(&mut self, key: &str) {
        match key {
            "pgdown" | "space" | "f" => self.page_down(),
            "pgup" | "b" => self.page_up(),
            "d" | "ctrl+d" => self.half_page_down(),
            "u" | "ctrl+u" => self.half_page_up(),
            "down" | "j" => self.scroll_down(1),
            "up" | "k" => self.scroll_up(1),
            "left" | "h" => {
                let step = self.horizontal_step as i64;
                self.set_x_offset(self.x_offset as i64 - step);
            }
            "right" | "l" => {
                let step = self.horizontal_step as i64;
                self.set_x_offset(self.x_offset as i64 + step);
            }
            _ => {}
        }
    }

    /// Renders the viewport, padded to its width and height.
    pub fn view(&self) -> String {
        if self.width == 0 || self.height == 0 {
            return String::new();
        }
        Style::new()
            .width(self.width)
            .height(self.height)
            .render(&self.visible_lines().join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(n: usize) -> Model {
        let mut m = Model::new();
        m.set_width(10);
        m.set_height(3);
        let content: Vec<String> = (0..n).map(|i| format!("line{i}")).collect();
        m.set_content(&content.join("\n"));
        m
    }

    #[test]
    fn empty_content_has_no_lines() {
        let mut m = Model::new();
        m.set_content("");
        assert_eq!(m.total_line_count(), 0);
        assert_eq!(m.scroll_percent(), 1.0);
    }

    #[test]
    fn scrolling_stops_at_the_ends() {
        let mut m = filled(10);
        assert!(m.at_top());
        m.scroll_up(1);
        assert_eq!(m.y_offset(), 0);
        m.goto_bottom();
        assert_eq!(m.y_offset(), 7);
        m.scroll_down(5);
        assert_eq!(m.y_offset(), 7);
        assert!(m.at_bottom());
    }

    #[test]
    fn half_and_whole_pages_move_by_the_height() {
        let mut m = filled(20);
        m.half_page_down();
        assert_eq!(m.y_offset(), 1);
        m.page_down();
        assert_eq!(m.y_offset(), 4);
        m.page_up();
        assert_eq!(m.y_offset(), 1);
        m.half_page_up();
        assert_eq!(m.y_offset(), 0);
    }

    #[test]
    fn scroll_percent_runs_from_zero_to_one() {
        let mut m = filled(13);
        assert_eq!(m.scroll_percent(), 0.0);
        m.goto_bottom();
        assert_eq!(m.scroll_percent(), 1.0);
        m.set_y_offset(5);
        assert_eq!(m.scroll_percent(), 0.5);
    }

    #[test]
    fn only_a_viewports_worth_of_lines_is_visible() {
        let mut m = filled(10);
        assert_eq!(m.visible_lines(), vec!["line0", "line1", "line2"]);
        m.set_y_offset(7);
        assert_eq!(m.visible_lines(), vec!["line7", "line8", "line9"]);
    }

    #[test]
    fn over_long_lines_are_cut_to_the_width() {
        let mut m = Model::new();
        m.set_width(4);
        m.set_height(1);
        m.set_content("abcdefgh");
        assert_eq!(m.visible_lines(), vec!["abcd"]);
    }

    #[test]
    fn the_view_is_padded_to_the_full_box() {
        let mut m = Model::new();
        m.set_width(6);
        m.set_height(3);
        m.set_content("ab");
        assert_eq!(m.view(), "ab    \n      \n      ");
    }

    #[test]
    fn shrinking_the_content_pulls_the_offset_back() {
        let mut m = filled(20);
        m.goto_bottom();
        assert_eq!(m.y_offset(), 17);
        m.set_content("a\nb\nc\nd");
        assert_eq!(m.y_offset(), 1);
    }
}
