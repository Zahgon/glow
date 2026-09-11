//! The file listing's mini and full help views.

use crate::deps::ansi;

use super::stash::{FilterState, StashModel, STASH_VIEW_HORIZONTAL_PADDING};
use super::styles::Styles;
use super::CommonModel;

/// One key and the action it performs.
struct HelpEntry {
    key: String,
    val: String,
}

/// A group of entries rendered as one column.
struct HelpColumn(Vec<HelpEntry>);

impl HelpColumn {
    /// Builds a column from key/value pairs.
    ///
    /// The pairs must be even; an odd list is a programming error, exactly as
    /// it is in the original.
    fn new(pairs: &[String]) -> HelpColumn {
        assert!(
            pairs.len() % 2 == 0,
            "help text group must have an even number of items"
        );
        HelpColumn(
            pairs
                .chunks(2)
                .map(|p| HelpEntry {
                    key: p[0].clone(),
                    val: p[1].clone(),
                })
                .collect(),
        )
    }

    /// Renders `height` rows, padding the short ones.
    fn render(&self, styles: &Styles, height: usize) -> Vec<String> {
        let (key_width, val_width) = self.max_widths();
        let mut rows = Vec::with_capacity(height);
        for i in 0..height {
            let (mut k, mut v) = (String::new(), String::new());
            if i < self.0.len() {
                k = self.0[i].key.clone();
                v = self.0[i].val.clone();
                if k == "s" {
                    k = styles.green_fg.render(&k);
                    v = styles.semi_dim_green_fg.render(&v);
                } else {
                    k = styles.gray_fg.render(&k);
                    v = styles.mid_gray_fg.render(&v);
                }
            }
            let mut b = String::new();
            b.push_str(&k);
            b.push_str(&" ".repeat(key_width.saturating_sub(ansi::string_width(&k))));
            b.push_str("  ");
            b.push_str(&v);
            b.push_str(&" ".repeat(val_width.saturating_sub(ansi::string_width(&v))));
            rows.push(b);
        }
        rows
    }

    /// The widest key and the widest value.
    fn max_widths(&self) -> (usize, usize) {
        let mut max_key = 0;
        let mut max_val = 0;
        for e in &self.0 {
            max_key = max_key.max(ansi::string_width(&e.key));
            max_val = max_val.max(ansi::string_width(&e.val));
        }
        (max_key, max_val)
    }
}

/// The shortest the expanded help can be.
const MIN_HELP_VIEW_HEIGHT: usize = 5;

fn words(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

impl StashModel {
    /// The help view and the number of lines it occupies.
    pub fn help_view(&self, common: &CommonModel) -> (String, usize) {
        let num_docs = self.visible_markdowns().len();

        // Help for while a filter is being typed.
        if self.filter_state == FilterState::Filtering {
            let h = match num_docs {
                0 => words(&["enter/esc", "cancel"]),
                1 => words(&["enter", "open", "esc", "cancel"]),
                _ => words(&[
                    "enter",
                    "confirm",
                    "esc",
                    "cancel",
                    "ctrl+j/ctrl+k ↑/↓",
                    "choose",
                ]),
            };
            return self.render_help(common, &[h]);
        }

        let mut nav_help: Vec<String> = Vec::new();
        let mut filter_help: Vec<String>;
        let selection_help: Vec<String> = Vec::new();
        let edit_help: Vec<String> = Vec::new();
        let section_help: Vec<String> = Vec::new();
        let mut app_help: Vec<String> = Vec::new();

        if num_docs > 0 && self.show_full_help {
            nav_help = words(&["enter", "open", "j/k ↑/↓", "choose"]);
        }

        if self.sections.len() > 1 {
            if self.show_full_help {
                nav_help.extend(words(&["tab/shift+tab", "section"]));
            } else {
                nav_help.extend(words(&["tab", "section"]));
            }
        }

        if self.paginator().total_pages > 1 {
            nav_help.extend(words(&["h/l ←/→", "page"]));
        }

        if self.filter_applied() {
            filter_help = words(&["/", "edit search", "esc", "clear filter"]);
        } else {
            filter_help = words(&["/", "find"]);
        }

        if self.err.is_some() {
            app_help.extend(words(&["!", "errors"]));
        }
        app_help.extend(words(&["r", "refresh"]));
        if num_docs > 0 {
            app_help.extend(words(&["e", "edit"]));
        }
        app_help.extend(words(&["q", "quit"]));

        if self.show_full_help {
            if self.filter_state != FilterState::Filtering {
                app_help.extend(words(&["?", "close help"]));
            }
            let mut selection_and_edit = selection_help.clone();
            selection_and_edit.extend(edit_help.clone());
            return self.render_help(
                common,
                &[
                    nav_help,
                    filter_help,
                    selection_and_edit,
                    section_help,
                    app_help,
                ],
            );
        }

        if self.filter_state != FilterState::Filtering {
            app_help.extend(words(&["?", "more"]));
        }
        let _ = &mut filter_help;
        self.render_help(
            common,
            &[
                nav_help,
                filter_help,
                selection_help,
                edit_help,
                section_help,
                app_help,
            ],
        )
    }

    /// Renders the help groups and reports how tall the result is.
    fn render_help(&self, common: &CommonModel, groups: &[Vec<String>]) -> (String, usize) {
        if self.show_full_help {
            let s = self.full_help_view(common, groups);
            let num_lines = s.matches('\n').count() + 1;
            return (s, num_lines.max(MIN_HELP_VIEW_HEIGHT));
        }
        let entries: Vec<String> = groups.iter().flatten().cloned().collect();
        (self.mini_help_view(common, &entries), 1)
    }

    /// One line of help, truncated rather than wrapped.
    fn mini_help_view(&self, common: &CommonModel, entries: &[String]) -> String {
        if entries.is_empty() {
            return String::new();
        }
        let styles = &common.styles;
        let truncation_char = styles.subtle_style.render("…");
        let truncation_width = ansi::string_width(&truncation_char);

        let left_gutter = "  ";
        let max_width = common.width as i64
            - STASH_VIEW_HORIZONTAL_PADDING as i64
            - truncation_width as i64
            - ansi::string_width(left_gutter) as i64;
        let mut s = left_gutter.to_string();

        let mut i = 0;
        while i < entries.len() {
            let k = styles.gray_fg.render(&entries[i]);
            let v = styles.mid_gray_fg.render(&entries[i + 1]);
            let mut next = format!("{k} {v}");
            if i < entries.len() - 2 {
                next.push_str(&styles.divider_dot.string());
            }
            if ansi::string_width(&s) as i64 + ansi::string_width(&next) as i64 >= max_width {
                s.push_str(&truncation_char);
                break;
            }
            s.push_str(&next);
            i += 2;
        }
        s
    }

    /// The expanded help, laid out in columns.
    fn full_help_view(&self, common: &CommonModel, groups: &[Vec<String>]) -> String {
        let columns: Vec<HelpColumn> = groups
            .iter()
            .filter(|g| !g.is_empty())
            .map(|g| HelpColumn::new(g))
            .collect();

        let tallest_col = columns.iter().map(|c| c.0.len()).max().unwrap_or(0);
        let rendered: Vec<Vec<String>> = columns
            .iter()
            .map(|c| c.render(&common.styles, tallest_col))
            .collect();

        merge_columns(&rendered)
    }
}

/// Merges rendered columns side by side.
fn merge_columns(cols: &[Vec<String>]) -> String {
    const MINIMUM_HEIGHT: usize = 3;

    let tallest = cols
        .iter()
        .map(|c| c.len())
        .max()
        .unwrap_or(0)
        .max(MINIMUM_HEIGHT);

    let mut b = String::new();
    for i in 0..tallest {
        for (j, col) in cols.iter().enumerate() {
            if i >= col.len() {
                continue;
            }
            if j == 0 {
                b.push_str("  "); // gutter
            } else {
                b.push_str("    "); // gap
            }
            b.push_str(&col[i]);
        }
        if i < tallest - 1 {
            b.push('\n');
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::markdown::Markdown;
    use crate::ui::Config;

    fn common() -> CommonModel {
        CommonModel {
            cfg: Config::default(),
            cwd: String::new(),
            width: 80,
            height: 24,
            styles: Styles::new(true),
        }
    }

    fn model(docs: usize) -> (CommonModel, StashModel) {
        let mut c = common();
        let mut m = StashModel::new(&c.styles);
        let mds: Vec<Markdown> = (0..docs)
            .map(|i| Markdown {
                note: format!("doc{i}.md"),
                ..Markdown::default()
            })
            .collect();
        m.add_markdowns(&c, mds);
        m.set_size(&mut c, 80, 24);
        (c, m)
    }

    #[test]
    fn the_mini_help_is_one_line() {
        let (c, m) = model(3);
        let (help, height) = m.help_view(&c);
        assert_eq!(height, 1);
        assert!(!help.contains('\n'));
        assert!(help.contains("find"), "{help}");
        assert!(help.contains("quit"), "{help}");
        assert!(help.contains("more"), "{help}");
    }

    #[test]
    fn the_full_help_is_at_least_five_lines() {
        let (c, mut m) = model(3);
        m.show_full_help = true;
        let (help, height) = m.help_view(&c);
        assert!(height >= 5, "{height}");
        assert!(help.contains("close help"), "{help}");
        assert!(help.contains("open"), "{help}");
    }

    #[test]
    fn edit_is_offered_only_with_documents() {
        let (c, m) = model(0);
        let (help, _) = m.help_view(&c);
        assert!(!help.contains("edit"), "{help}");
        let (c, m) = model(1);
        let (help, _) = m.help_view(&c);
        assert!(help.contains("edit"), "{help}");
    }

    #[test]
    fn errors_add_their_own_binding() {
        let (c, mut m) = model(1);
        m.err = Some("boom".into());
        let (help, _) = m.help_view(&c);
        assert!(help.contains("errors"), "{help}");
    }

    #[test]
    fn filtering_replaces_the_help_entirely() {
        let (c, mut m) = model(3);
        m.filter_state = FilterState::Filtering;
        m.filtered_markdowns = vec![0, 1];
        let (help, _) = m.help_view(&c);
        assert!(help.contains("confirm"), "{help}");
        assert!(help.contains("cancel"), "{help}");
        assert!(!help.contains("quit"), "{help}");
    }

    #[test]
    fn a_single_filtered_result_offers_open() {
        let (c, mut m) = model(3);
        m.filter_state = FilterState::Filtering;
        m.filtered_markdowns = vec![0];
        let (help, _) = m.help_view(&c);
        assert!(help.contains("open"), "{help}");
    }

    #[test]
    fn columns_are_gutter_then_gap_separated() {
        let cols = vec![
            vec!["a".to_string(), "b".to_string()],
            vec!["c".to_string(), "d".to_string()],
        ];
        assert_eq!(merge_columns(&cols), "  a    c\n  b    d\n");
    }
}
