//! Table layout.
//!
//! A port of the column-sizing and drawing behaviour of `lipgloss/table`, which
//! Glamour drives with column borders only, a header rule, and a one-space
//! horizontal margin on every cell.

use crate::deps::ansi;
use crate::deps::lipgloss;

/// The glyphs used to draw a table.
#[derive(Debug, Clone)]
pub struct Border {
    /// Horizontal rule character.
    pub top: String,
    /// Vertical column separator.
    pub left: String,
    /// Crossing of a rule and a column separator.
    pub middle: String,
}

impl Default for Border {
    /// `lipgloss.NormalBorder`, used when a style leaves the separators unset.
    fn default() -> Self {
        Border {
            top: "─".into(),
            left: "│".into(),
            middle: "┼".into(),
        }
    }
}

/// Horizontal alignment of a column's cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// Flush left.
    Left,
    /// Centred.
    Center,
    /// Flush right.
    Right,
}

/// A table ready to be laid out.
#[derive(Debug, Clone)]
pub struct Table {
    /// Target total width.
    pub width: usize,
    /// Whether over-long cells wrap instead of being truncated.
    pub wrap: bool,
    /// Header cells; empty when the table has no header.
    pub headers: Vec<String>,
    /// Body rows.
    pub rows: Vec<Vec<String>>,
    /// Per-column alignment.
    pub aligns: Vec<Align>,
    /// Separator glyphs.
    pub border: Border,
    /// Horizontal padding inside a cell, one column on each side.
    pub margin: usize,
}

impl Table {
    /// A table with Glamour's defaults: column borders, a header rule, and a
    /// one-column margin on each side of every cell.
    pub fn new(width: usize) -> Self {
        Table {
            width,
            wrap: true,
            headers: Vec::new(),
            rows: Vec::new(),
            aligns: Vec::new(),
            border: Border::default(),
            margin: 1,
        }
    }

    fn column_count(&self) -> usize {
        let mut n = self.headers.len();
        for r in &self.rows {
            n = n.max(r.len());
        }
        n
    }

    fn all_rows(&self) -> Vec<Vec<String>> {
        let mut all = Vec::new();
        if !self.headers.is_empty() {
            all.push(self.headers.clone());
        }
        all.extend(self.rows.iter().cloned());
        all
    }

    /// The horizontal frame each cell contributes: one margin column per side.
    fn x_padding(&self) -> usize {
        self.margin * 2
    }

    /// The intrinsic width of the table, used when no width was requested.
    fn detect_width(&self) -> usize {
        let cols = self.column_count();
        let all = self.all_rows();
        let mut chars = 0usize;
        for j in 0..cols {
            chars += all
                .iter()
                .map(|r| r.get(j).map(|c| ansi::string_width(c)).unwrap_or(0))
                .max()
                .unwrap_or(0);
        }
        chars + cols * self.x_padding() + cols.saturating_sub(1)
    }

    /// Column widths after expansion or shrinking to the target width.
    fn widths(&self) -> Vec<usize> {
        let cols = self.column_count();
        if cols == 0 {
            return Vec::new();
        }
        let target = if self.width == 0 {
            self.detect_width()
        } else {
            self.width
        };
        let all = self.all_rows();

        let mut maxes = vec![0usize; cols];
        let mut per_col: Vec<Vec<usize>> = vec![Vec::new(); cols];
        for (i, row) in all.iter().enumerate() {
            for j in 0..cols {
                let w = row.get(j).map(|c| ansi::string_width(c)).unwrap_or(0);
                maxes[j] = maxes[j].max(w);
                // The first row seeds min/max; later rows also feed the median.
                if i > 0 {
                    per_col[j].push(w);
                }
            }
        }
        let mins: Vec<usize> = (0..cols)
            .map(|j| {
                all.iter()
                    .map(|r| r.get(j).map(|c| ansi::string_width(c)).unwrap_or(0))
                    .min()
                    .unwrap_or(0)
            })
            .collect();
        let medians: Vec<usize> = per_col.iter().map(|w| median(w)).collect();

        let pad = self.x_padding();
        let mut widths: Vec<usize> = maxes.iter().map(|m| m + pad).collect();
        let border_total = cols.saturating_sub(1); // one column separator per gap

        if widths.iter().sum::<usize>() + border_total <= target {
            // Expand: keep widening the narrowest column until we fill the width.
            loop {
                let total: usize = widths.iter().sum::<usize>() + border_total;
                if total >= target {
                    break;
                }
                let mut idx = 0;
                let mut narrowest = usize::MAX;
                for (j, w) in widths.iter().enumerate() {
                    if *w < narrowest {
                        narrowest = *w;
                        idx = j;
                    }
                }
                widths[idx] += 1;
            }
            return widths;
        }

        let min_width = |j: usize| pad + mins[j].max(1);

        let shrink_biggest = |widths: &mut Vec<usize>, very_big_only: bool, use_floor: bool| loop {
            let total: usize = widths.iter().sum::<usize>() + border_total;
            if total <= target {
                break;
            }
            let mut big_index: i64 = -1;
            let mut big_width: i64 = -1;
            for (j, w) in widths.iter().enumerate() {
                if use_floor && *w <= min_width(j) {
                    continue;
                }
                if very_big_only {
                    if *w >= target / 2 && (*w as i64) > big_width {
                        big_width = *w as i64;
                        big_index = j as i64;
                    }
                } else if (*w as i64) > big_width {
                    big_width = *w as i64;
                    big_index = j as i64;
                }
            }
            if big_index < 0 || widths[big_index as usize] == 0 {
                break;
            }
            widths[big_index as usize] -= 1;
        };

        let shrink_to_median = |widths: &mut Vec<usize>, use_floor: bool| loop {
            let total: usize = widths.iter().sum::<usize>() + border_total;
            if total <= target {
                break;
            }
            let mut best_diff: i64 = -1;
            let mut best_index: i64 = -1;
            for (j, w) in widths.iter().enumerate() {
                if use_floor && *w <= min_width(j) {
                    continue;
                }
                let diff = *w as i64 - medians[j] as i64;
                if diff > 0 && diff > best_diff {
                    best_diff = diff;
                    best_index = j as i64;
                }
            }
            if best_index <= 0 || widths[best_index as usize] == 0 {
                break;
            }
            widths[best_index as usize] -= 1;
        };

        shrink_biggest(&mut widths, true, true);
        shrink_to_median(&mut widths, true);
        shrink_biggest(&mut widths, false, true);

        if widths.iter().sum::<usize>() + border_total > target {
            shrink_biggest(&mut widths, true, false);
            shrink_to_median(&mut widths, false);
            shrink_biggest(&mut widths, false, false);
        }

        widths
    }

    /// Renders the table.
    pub fn render(&self) -> String {
        let cols = self.column_count();
        if cols == 0 || (self.headers.is_empty() && self.rows.is_empty()) {
            return String::new();
        }
        let widths = self.widths();
        let mut out: Vec<String> = Vec::new();

        if !self.headers.is_empty() {
            let mut header = self.headers.clone();
            header.resize(cols, String::new());
            out.extend(self.render_row(&header, &widths, true));
            // Header rule.
            let mut rule = String::new();
            for (j, w) in widths.iter().enumerate() {
                rule.push_str(&self.border.top.repeat(*w));
                if j + 1 < widths.len() {
                    rule.push_str(&self.border.middle);
                }
            }
            out.push(rule);
        }
        for row in &self.rows {
            let mut r = row.clone();
            r.resize(cols, String::new());
            out.extend(self.render_row(&r, &widths, false));
        }

        let joined = out.join("\n");
        if self.width == 0 {
            return joined;
        }
        // Table.String() finishes with MaxWidth(width).
        joined
            .split('\n')
            .map(|l| ansi::truncate_with_tail(l, self.width, ""))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render_row(&self, row: &[String], widths: &[usize], header: bool) -> Vec<String> {
        let inner: Vec<usize> = widths
            .iter()
            .map(|w| w.saturating_sub(self.x_padding()))
            .collect();

        // Lay each cell out into its own lines, then join horizontally.
        let mut cells: Vec<Vec<String>> = Vec::with_capacity(row.len());
        let mut height = 1usize;
        for (j, cell) in row.iter().enumerate() {
            let w = inner.get(j).copied().unwrap_or(0);
            let lines = if header || !self.wrap {
                vec![ansi::truncate_with_tail(cell, w, "…")]
            } else if w == 0 {
                // Width(0) disables wrapping; the cell is cut by MaxWidth below.
                vec![cell.clone()]
            } else {
                lipgloss::wrap(cell, w as i64, "")
                    .split('\n')
                    .map(str::to_string)
                    .collect()
            };
            height = height.max(lines.len());
            cells.push(lines);
        }

        let align = |j: usize| self.aligns.get(j).copied().unwrap_or(Align::Left);
        let mut lines = Vec::with_capacity(height);
        for i in 0..height {
            let mut line = String::new();
            for (j, cell) in cells.iter().enumerate() {
                if j > 0 {
                    line.push_str(&self.border.left);
                }
                let content = cell.get(i).cloned().unwrap_or_default();
                let w = inner.get(j).copied().unwrap_or(0);
                let mut cell_text = String::new();
                cell_text.push_str(&" ".repeat(self.margin));
                cell_text.push_str(&pad_to(&content, w, align(j)));
                cell_text.push_str(&" ".repeat(self.margin));
                let full = widths.get(j).copied().unwrap_or(0);
                line.push_str(&ansi::truncate_with_tail(&cell_text, full, ""));
            }
            lines.push(line);
        }
        lines
    }
}

fn pad_to(s: &str, width: usize, align: Align) -> String {
    let w = ansi::string_width(s);
    if w >= width {
        return s.to_string();
    }
    let short = width - w;
    match align {
        Align::Left => format!("{}{}", s, " ".repeat(short)),
        Align::Right => format!("{}{}", " ".repeat(short), s),
        Align::Center => {
            let left = short / 2;
            format!("{}{}{}", " ".repeat(left), s, " ".repeat(short - left))
        }
    }
}

fn median(values: &[usize]) -> usize {
    if values.is_empty() {
        return 0;
    }
    let mut v = values.to_vec();
    v.sort_unstable();
    if v.len() % 2 == 0 {
        let h = v.len() / 2;
        (v[h - 1] + v[h]) / 2
    } else {
        v[v.len() / 2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notty_border() -> Border {
        Border {
            top: "-".into(),
            left: "|".into(),
            middle: "|".into(),
        }
    }

    #[test]
    fn expands_narrow_tables_to_the_target_width() {
        let mut t = Table::new(76);
        t.border = notty_border();
        t.headers = vec!["a".into(), "b".into()];
        t.rows = vec![vec!["1".into(), "2".into()]];
        t.aligns = vec![Align::Left, Align::Left];
        let out = t.render();
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines.len(), 3);
        for l in &lines {
            assert_eq!(ansi::string_width(l), 76, "line {l:?} is the wrong width");
        }
        assert_eq!(lines[1], format!("{}|{}", "-".repeat(38), "-".repeat(37)));
        assert!(lines[0].starts_with(" a"));
    }

    #[test]
    fn shrinks_wide_tables_to_the_target_width() {
        let mut t = Table::new(20);
        t.border = notty_border();
        t.headers = vec!["a".into(), "b".into()];
        t.rows = vec![vec!["x".repeat(40), "y".into()]];
        let out = t.render();
        for l in out.split('\n') {
            assert!(ansi::string_width(l) <= 20);
        }
    }

    #[test]
    fn empty_table_renders_nothing() {
        assert_eq!(Table::new(40).render(), "");
    }
}
