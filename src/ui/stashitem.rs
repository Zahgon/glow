//! Rendering one entry of the file listing, and the listing around it.

use crate::deps::ansi;
use crate::deps::bubbles::paginator;
use crate::deps::humanize;
use crate::deps::lipgloss::Style;

use super::markdown::Markdown;
use super::stash::{
    stashing_status_message, style_filtered_text, FilterState, SectionKey, StashModel,
    StashViewState, STASH_INDENT, STASH_VIEW_BOTTOM_PADDING, STASH_VIEW_HORIZONTAL_PADDING,
    STASH_VIEW_ITEM_HEIGHT, STASH_VIEW_TOP_PADDING,
};
use super::styles::Styles;
use super::{error_view, indent, CommonModel, ELLIPSIS};

/// The gutter drawn beside the selected entry.
const VERTICAL_LINE: &str = "│";

/// Renders one entry into `b`.
pub fn stash_item_view(
    b: &mut String,
    m: &StashModel,
    common: &CommonModel,
    index: usize,
    md: &Markdown,
) {
    let truncate_to = common
        .width
        .saturating_sub(STASH_VIEW_HORIZONTAL_PADDING * 2);
    let styles = &common.styles;

    let mut title = ansi::truncate_with_tail(&md.note, truncate_to, ELLIPSIS);
    let mut date = md.relative_time();
    let gutter;
    // Upstream keeps the icon and separator empty; the columns are still laid
    // out around them.
    let mut icon = String::new();
    let mut separator = String::new();

    let is_selected = index as i64 == m.cursor();
    let is_filtering = m.filter_state == FilterState::Filtering;
    let single_filtered_item = is_filtering && m.visible_markdowns().len() == 1;

    // With several filtered results nothing is highlighted; with exactly one,
    // it is, because return will open it.
    if (is_selected && !is_filtering) || single_filtered_item {
        if m.show_status_message && m.status_message == stashing_status_message() {
            gutter = styles.green_fg.render(VERTICAL_LINE);
            icon = styles.dim_green_fg.render(&icon);
            title = styles.green_fg.render(&title);
            date = styles.semi_dim_green_fg.render(&date);
            separator = styles.semi_dim_green_fg.render(&separator);
        } else {
            gutter = styles.dull_fuchsia_fg.render(VERTICAL_LINE);
            if (m.current_section().key == SectionKey::Filter
                && m.filter_state == FilterState::FilterApplied)
                || single_filtered_item
            {
                let s = Style::new().foreground(styles.fuchsia.clone());
                title = style_filtered_text(
                    &title,
                    &m.filter_input.value(),
                    &s,
                    &s.clone().underline(true),
                );
            } else {
                title = styles.fuchsia_fg.render(&title);
                icon = styles.fuchsia_fg.render(&icon);
            }
            date = styles.dim_fuchsia_fg.render(&date);
            separator = styles.dull_fuchsia_fg.render(&separator);
        }
    } else {
        gutter = " ".to_string();
        if m.show_status_message && m.status_message == stashing_status_message() {
            icon = styles.dim_green_fg.render(&icon);
            title = styles.green_fg.render(&title);
            date = styles.semi_dim_green_fg.render(&date);
            separator = styles.semi_dim_green_fg.render(&separator);
        } else if is_filtering && m.filter_input.value().is_empty() {
            icon = styles.dim_green_fg.render(&icon);
            title = styles.dim_normal_fg.render(&title);
            date = styles.dim_bright_gray_fg.render(&date);
            separator = styles.dim_bright_gray_fg.render(&separator);
        } else {
            icon = styles.green_fg.render(&icon);
            let s = Style::new().foreground(styles.adaptive("#1a1a1a", "#dddddd"));
            title = style_filtered_text(
                &title,
                &m.filter_input.value(),
                &s,
                &s.clone().underline(true),
            );
            date = styles.gray_fg.render(&date);
            separator = styles.bright_gray_fg.render(&separator);
        }
    }

    b.push_str(&format!("{gutter} {icon}{separator}{separator}{title}\n"));
    b.push_str(&format!("{gutter} {date}"));
}

/// The ` Glow ` logo.
pub fn glow_logo_view(styles: &Styles) -> String {
    styles.logo_style.render(" Glow ")
}

impl StashModel {
    /// Renders the whole listing.
    pub fn view(&self, common: &CommonModel) -> String {
        let styles = &common.styles;
        let mut s = String::new();

        match self.view_state {
            StashViewState::ShowingError => {
                return error_view(styles, self.err.as_deref().unwrap_or_default(), false)
            }
            StashViewState::LoadingDocument => {
                s.push_str(&format!(" {} Loading document...", self.spinner.view()));
            }
            StashViewState::Ready => {
                let loading_indicator = if self.should_spin() {
                    self.spinner.view()
                } else {
                    " ".to_string()
                };

                let header = self.header_view(common);

                let mut logo_or_filter = " ".to_string();
                if self.show_status_message && self.filter_state == FilterState::Filtering {
                    logo_or_filter.push_str(&self.status_message.render(styles));
                } else if self.filter_state == FilterState::Filtering {
                    logo_or_filter.push_str(&self.filter_input.view());
                } else {
                    logo_or_filter.push_str(&glow_logo_view(styles));
                    if self.show_status_message {
                        logo_or_filter.push_str("  ");
                        logo_or_filter.push_str(&self.status_message.render(styles));
                    }
                }
                let logo_or_filter = ansi::truncate_with_tail(
                    &logo_or_filter,
                    common.width.saturating_sub(1),
                    ELLIPSIS,
                );

                let (help, help_height) = self.help_view(common);

                let populated_view = self.populated_view(common);
                let populated_view_height = populated_view.matches('\n').count() + 2;

                // Empty height is filled with newlines so the footer reaches
                // the bottom.
                let avail_height = common.height as i64
                    - STASH_VIEW_TOP_PADDING as i64
                    - populated_view_height as i64
                    - help_height as i64
                    - STASH_VIEW_BOTTOM_PADDING as i64;
                let blank_lines = "\n".repeat(avail_height.max(0) as usize);

                let mut pagination = String::new();
                if self.paginator().total_pages > 1 {
                    pagination = self.paginator().view();
                    // A dot paginator wider than the window falls back to the
                    // arabic one.
                    if ansi::string_width(&pagination)
                        > common.width.saturating_sub(STASH_VIEW_HORIZONTAL_PADDING)
                    {
                        let mut p = self.paginator().clone();
                        p.kind = paginator::Type::Arabic;
                        pagination = styles.pagination_style.render(&p.view());
                    }
                }

                s.push_str(&format!(
                    "{loading_indicator}{logo_or_filter}\n\n  {header}\n\n{populated_view}\n\n{blank_lines}  {pagination}\n\n{help}"
                ));
            }
        }
        format!("\n{}", indent(&s, STASH_INDENT))
    }

    /// The counts and tabs above the listing.
    pub fn header_view(&self, common: &CommonModel) -> String {
        let styles = &common.styles;
        let local_count = self.markdowns.len();
        let mut sections: Vec<String> = Vec::new();

        // Filter results.
        if self.filter_state == FilterState::Filtering {
            if local_count == 0 {
                return styles.gray_fg.render("Nothing found.");
            }
            sections.push(styles.gray_fg.render(&format!("{local_count} local")));
            return sections.join(&styles.divider_dot.string());
        }

        // Tabs.
        for (i, v) in self.sections.iter().enumerate() {
            let mut s = match v.key {
                SectionKey::Documents => humanize::plural(local_count as i64, "document", ""),
                SectionKey::Filter => format!(
                    "{} \u{201c}{}\u{201d}",
                    self.filtered_markdowns.len(),
                    self.filter_input.value()
                ),
            };
            s = if self.section_index == i && self.sections.len() > 1 {
                styles.selected_tab_style.render(&s)
            } else {
                styles.tab_style.render(&s)
            };
            sections.push(s);
        }

        sections.join(&styles.divider_bar.string())
    }

    /// The entries on the current page, padded out to a full page.
    pub fn populated_view(&self, common: &CommonModel) -> String {
        let mds = self.visible_markdowns();
        let mut b = String::new();

        if mds.is_empty() {
            match self.sections[self.section_index].key {
                SectionKey::Documents => {
                    let text = if self.loading_done() {
                        "No files found."
                    } else {
                        "Looking for local files..."
                    };
                    b.push_str("  ");
                    b.push_str(&common.styles.gray_fg.render(text));
                }
                SectionKey::Filter => return String::new(),
            }
        }

        if !mds.is_empty() {
            let (start, end) = self.paginator().slice_bounds(mds.len());
            let docs = &mds[start..end];
            for (i, md) in docs.iter().enumerate() {
                stash_item_view(&mut b, self, common, i, md);
                if i != docs.len() - 1 {
                    b.push_str("\n\n");
                }
            }
        }

        // Pad the last page out so the footer stays put.
        let items_on_page = self.paginator().items_on_page(mds.len());
        if items_on_page < self.paginator().per_page {
            let mut n = (self.paginator().per_page - items_on_page) * STASH_VIEW_ITEM_HEIGHT;
            if mds.is_empty() {
                n -= STASH_VIEW_ITEM_HEIGHT - 1;
            }
            for _ in 0..n {
                b.push('\n');
            }
        }

        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                modtime: Some(std::time::SystemTime::now()),
                ..Markdown::default()
            })
            .collect();
        m.add_markdowns(&c, mds);
        m.set_size(&mut c, 80, 24);
        (c, m)
    }

    #[test]
    fn an_entry_is_two_lines_of_title_and_date() {
        let (c, m) = model(3);
        let mut b = String::new();
        stash_item_view(&mut b, &m, &c, 0, &m.markdowns[0]);
        let lines: Vec<&str> = b.split('\n').collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("doc0.md"), "{b}");
        assert!(lines[1].contains("just now"), "{b}");
    }

    #[test]
    fn the_selected_entry_has_a_gutter() {
        let (c, m) = model(3);
        let mut selected = String::new();
        stash_item_view(&mut selected, &m, &c, 0, &m.markdowns[0]);
        let mut other = String::new();
        stash_item_view(&mut other, &m, &c, 1, &m.markdowns[1]);
        assert!(selected.contains(VERTICAL_LINE), "{selected}");
        assert!(!other.contains(VERTICAL_LINE), "{other}");
    }

    #[test]
    fn the_header_pluralises_the_document_count() {
        let (c, m) = model(1);
        assert!(m.header_view(&c).contains("1 document"));
        let (c, m) = model(3);
        assert!(m.header_view(&c).contains("3 documents"));
        let (c, m) = model(0);
        assert!(m.header_view(&c).contains("0 documents"));
    }

    #[test]
    fn filtering_shows_the_local_count() {
        let (c, mut m) = model(4);
        m.filter_state = FilterState::Filtering;
        assert!(
            m.header_view(&c).contains("4 local"),
            "{}",
            m.header_view(&c)
        );
    }

    #[test]
    fn an_empty_listing_says_so() {
        let (c, mut m) = model(0);
        assert!(m.populated_view(&c).contains("Looking for local files..."));
        m.loaded = true;
        assert!(m.populated_view(&c).contains("No files found."));
    }

    #[test]
    fn the_listing_is_indented_and_starts_with_a_blank_line() {
        let (c, m) = model(3);
        let view = m.view(&c);
        assert!(view.starts_with('\n'));
        assert!(view.contains("Glow"));
        assert!(view.contains("3 documents"));
    }

    #[test]
    fn a_page_is_padded_to_a_fixed_height() {
        let (c, m) = model(2);
        let view = m.populated_view(&c);
        // Two entries of two lines each, a blank line between them, then
        // padding for the three empty slots on the page.
        assert_eq!(view.matches('\n').count(), 4 + 3 * 3);
    }
}
