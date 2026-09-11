//! The file listing: sections, pagination, filtering and its keys.

use crate::deps::ansi;
use crate::deps::bubbles::{paginator, spinner, textinput};
use crate::deps::bubbletea::Cmd;
use crate::deps::fuzzy;

use super::markdown::{normalize, Markdown};
use super::sort::sort_markdowns;
use super::styles::Styles;
use super::{CommonModel, Msg};

/// Left indent of the whole listing.
pub const STASH_INDENT: usize = 1;
/// Height of one entry, including the gap under it.
pub const STASH_VIEW_ITEM_HEIGHT: usize = 3;
/// Logo, status bar and gaps above the entries.
pub const STASH_VIEW_TOP_PADDING: usize = 5;
/// Pagination and gaps below the entries, not counting help.
pub const STASH_VIEW_BOTTOM_PADDING: usize = 3;
/// Horizontal padding around the entries.
pub const STASH_VIEW_HORIZONTAL_PADDING: usize = 6;

/// The message shown while a document is being stashed.
pub fn stashing_status_message() -> StatusMessage {
    StatusMessage {
        status: StatusMessageType::Normal,
        message: "Stashing...".to_string(),
    }
}

/// The high-level state of the file listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StashViewState {
    /// Showing the listing.
    #[default]
    Ready,
    /// Waiting for a document to load.
    LoadingDocument,
    /// Showing an error.
    ShowingError,
}

/// The kind of documents a section shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKey {
    /// Every document found.
    Documents,
    /// The current filter's results.
    Filter,
}

/// A tab and the state of its contents.
#[derive(Debug, Clone)]
pub struct Section {
    /// Which documents the section shows.
    pub key: SectionKey,
    /// Pagination state.
    pub paginator: paginator::Model,
    /// Selected row on the current page.
    pub cursor: i64,
}

impl Section {
    fn new(key: SectionKey) -> Section {
        Section {
            key,
            paginator: paginator::Model::dots(),
            cursor: 0,
        }
    }
}

/// Whether a filter is being typed, applied, or absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterState {
    /// No filter set.
    #[default]
    Unfiltered,
    /// The user is typing a filter.
    Filtering,
    /// A filter is applied and the user is not editing it.
    FilterApplied,
}

/// How a status message should read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusMessageType {
    /// Ordinary.
    #[default]
    Normal,
    /// Quiet.
    Subtle,
    /// A failure.
    Error,
}

/// An ephemeral note shown in the listing's header.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusMessage {
    /// How it reads.
    pub status: StatusMessageType,
    /// The text.
    pub message: String,
}

impl StatusMessage {
    /// Renders the message for the given styles.
    pub fn render(&self, styles: &Styles) -> String {
        match self.status {
            StatusMessageType::Subtle => styles.dim_green_fg.render(&self.message),
            StatusMessageType::Error => styles.red_fg.render(&self.message),
            StatusMessageType::Normal => styles.green_fg.render(&self.message),
        }
    }
}

/// The file listing.
pub struct StashModel {
    /// The last error, shown by `!`.
    pub err: Option<String>,
    /// The loading spinner.
    pub spinner: spinner::Model,
    /// The filter input.
    pub filter_input: textinput::Model,
    /// The listing's state.
    pub view_state: StashViewState,
    /// The filter's state.
    pub filter_state: FilterState,
    /// Whether the expanded help is shown.
    pub show_full_help: bool,
    /// Whether a status message is showing.
    pub show_status_message: bool,
    /// The status message.
    pub status_message: StatusMessage,
    /// Sections the user can cycle through; order matters, so this is a list.
    pub sections: Vec<Section>,
    /// Index of the section being looked at.
    pub section_index: usize,
    /// Whether the document search has finished.
    pub loaded: bool,
    /// Every document found.
    pub markdowns: Vec<Markdown>,
    /// Indexes of the documents the filter currently selects.
    pub filtered_markdowns: Vec<usize>,
}

impl StashModel {
    /// A listing with one section and a focused filter input.
    pub fn new(styles: &Styles) -> StashModel {
        let mut sp = spinner::Model::new();
        sp.spinner = spinner::LINE;
        sp.style = styles.stash_spinner_style.clone();

        let mut si = textinput::Model::new();
        si.prompt = "Find:".to_string();
        si.styles.prompt = styles.stash_input_prompt_style.clone();
        si.styles.cursor = crate::deps::lipgloss::Style::new().foreground(styles.fuchsia.clone());
        si.focus();

        let mut m = StashModel {
            err: None,
            spinner: sp,
            filter_input: si,
            view_state: StashViewState::Ready,
            filter_state: FilterState::Unfiltered,
            show_full_help: false,
            show_status_message: false,
            status_message: StatusMessage::default(),
            sections: vec![Section::new(SectionKey::Documents)],
            section_index: 0,
            loaded: false,
            markdowns: Vec::new(),
            filtered_markdowns: Vec::new(),
        };
        m.style_paginators(styles);
        m
    }

    /// Applies the current styles to every paginator.
    pub fn style_paginators(&mut self, styles: &Styles) {
        for s in &mut self.sections {
            s.paginator.active_dot = styles.bright_gray_fg.render("•");
            s.paginator.inactive_dot = styles.dark_gray_fg.render("•");
        }
    }

    /// Whether the document search has finished.
    pub fn loading_done(&self) -> bool {
        self.loaded
    }

    /// The section being looked at.
    pub fn current_section(&self) -> &Section {
        &self.sections[self.section_index]
    }

    fn current_section_mut(&mut self) -> &mut Section {
        &mut self.sections[self.section_index]
    }

    /// The current section's pagination.
    pub fn paginator(&self) -> &paginator::Model {
        &self.current_section().paginator
    }

    fn paginator_mut(&mut self) -> &mut paginator::Model {
        &mut self.current_section_mut().paginator
    }

    /// The selected row on the current page.
    pub fn cursor(&self) -> i64 {
        self.current_section().cursor
    }

    /// Selects a row on the current page.
    pub fn set_cursor(&mut self, i: i64) {
        self.current_section_mut().cursor = i;
    }

    /// Whether the spinner should be animating.
    pub fn should_spin(&self) -> bool {
        !self.loading_done() || self.view_state == StashViewState::LoadingDocument
    }

    /// Resizes the listing.
    pub fn set_size(&mut self, common: &mut CommonModel, width: usize, height: usize) {
        common.width = width;
        common.height = height;
        let prompt_width = ansi::string_width(&self.filter_input.prompt);
        self.filter_input.set_width(
            width as i64 - STASH_VIEW_HORIZONTAL_PADDING as i64 * 2 - prompt_width as i64,
        );
        self.update_pagination(common);
    }

    /// Drops the filter and the section it created.
    pub fn reset_filtering(&mut self, common: &CommonModel) {
        self.filter_state = FilterState::Unfiltered;
        self.filter_input.reset();
        self.filtered_markdowns.clear();

        sort_markdowns(&mut self.markdowns);

        // The filtered section is always last, so it is sliced off the end.
        if self.sections.last().map(|s| s.key) == Some(SectionKey::Filter) {
            self.sections.pop();
        }
        if self.section_index > self.sections.len() - 1 {
            self.section_index = 0;
        }
        self.update_pagination(common);
    }

    /// Whether a filter is in play.
    pub fn filter_applied(&self) -> bool {
        self.filter_state != FilterState::Unfiltered
    }

    /// Whether the filter results need recomputing.
    pub fn should_update_filter(&self) -> bool {
        self.filter_applied()
    }

    /// Recomputes pagination for the current contents.
    pub fn update_pagination(&mut self, common: &CommonModel) {
        let (_, help_height) = self.help_view(common);

        let available_height = common.height as i64
            - STASH_VIEW_TOP_PADDING as i64
            - help_height as i64
            - STASH_VIEW_BOTTOM_PADDING as i64;

        let per_page = (available_height / STASH_VIEW_ITEM_HEIGHT as i64).max(1);
        let visible = self.visible_count();
        let p = self.paginator_mut();
        p.per_page = per_page as usize;
        if visible < 1 {
            p.total_pages = 1;
        } else {
            p.set_total_pages(visible);
        }
        // Keep the page in bounds.
        if p.page + 1 >= p.total_pages {
            p.page = p.total_pages.saturating_sub(1);
        }
    }

    /// Index of the selected document within the visible set.
    pub fn markdown_index(&self) -> i64 {
        let p = self.paginator();
        (p.page * p.per_page) as i64 + self.cursor()
    }

    /// The selected document, if there is one.
    pub fn selected_markdown(&self) -> Option<&Markdown> {
        let i = self.markdown_index();
        let visible = self.visible_markdowns();
        if i < 0 || visible.is_empty() || visible.len() as i64 <= i {
            return None;
        }
        Some(visible[i as usize])
    }

    /// Adds documents to the listing.
    pub fn add_markdowns(&mut self, common: &CommonModel, mds: Vec<Markdown>) {
        if mds.is_empty() {
            return;
        }
        self.markdowns.extend(mds);
        if !self.filter_applied() {
            sort_markdowns(&mut self.markdowns);
        }
        self.update_pagination(common);
    }

    /// The documents that should currently be shown.
    pub fn visible_markdowns(&self) -> Vec<&Markdown> {
        if self.filter_state == FilterState::Filtering
            || self.current_section().key == SectionKey::Filter
        {
            return self
                .filtered_markdowns
                .iter()
                .filter_map(|i| self.markdowns.get(*i))
                .collect();
        }
        self.markdowns.iter().collect()
    }

    fn visible_count(&self) -> usize {
        if self.filter_state == FilterState::Filtering
            || self.current_section().key == SectionKey::Filter
        {
            return self.filtered_markdowns.len();
        }
        self.markdowns.len()
    }

    /// Opens a document in the pager.
    pub fn open_markdown(&mut self, md: &Markdown) -> Cmd<Msg> {
        self.view_state = StashViewState::LoadingDocument;
        Cmd::batch(vec![
            load_local_markdown(md),
            Cmd::tick(self.spinner.fps(), Msg::SpinnerTick),
        ])
    }

    /// Hides the status message.
    pub fn hide_status_message(&mut self) {
        self.show_status_message = false;
        self.status_message = StatusMessage::default();
    }

    /// Moves the selection up, paging back at the top of a page.
    pub fn move_cursor_up(&mut self) {
        self.set_cursor(self.cursor() - 1);
        if self.cursor() < 0 && self.paginator().page == 0 {
            self.set_cursor(0);
            return;
        }
        if self.cursor() >= 0 {
            return;
        }
        self.paginator_mut().prev_page();
        let items = self.paginator().items_on_page(self.visible_count()) as i64;
        self.set_cursor(items - 1);
    }

    /// Moves the selection down, paging forward at the end of a page.
    pub fn move_cursor_down(&mut self) {
        let items_on_page = self.paginator().items_on_page(self.visible_count()) as i64;

        self.set_cursor(self.cursor() + 1);
        if self.cursor() < items_on_page {
            return;
        }
        if !self.paginator().on_last_page() {
            self.paginator_mut().next_page();
            self.set_cursor(0);
            return;
        }
        // While filtering the cursor can overshoot the page; starting from the
        // top reads better than clamping to the end.
        if self.cursor() > items_on_page {
            self.set_cursor(0);
            return;
        }
        self.set_cursor(items_on_page - 1);
    }

    /// Folds a message into the listing.
    pub fn update(&mut self, common: &mut CommonModel, msg: &Msg) -> Cmd<Msg> {
        let mut cmds: Vec<Cmd<Msg>> = Vec::new();

        match msg {
            Msg::Err(e) => self.err = Some(e.clone()),
            Msg::LocalFileSearchFinished => self.loaded = true,
            Msg::FilteredMarkdown(indexes) => {
                self.filtered_markdowns = indexes.clone();
                self.set_cursor(0);
                return Cmd::None;
            }
            Msg::SpinnerTick => {
                if self.should_spin() {
                    self.spinner.tick();
                    cmds.push(Cmd::tick(self.spinner.fps(), Msg::SpinnerTick));
                }
            }
            Msg::StatusMessageTimeout(super::AppContext::Stash) => self.hide_status_message(),
            _ => {}
        }

        if self.filter_state == FilterState::Filtering {
            cmds.push(self.handle_filtering(common, msg));
            return Cmd::batch(cmds);
        }

        match self.view_state {
            StashViewState::Ready => cmds.push(self.handle_document_browsing(common, msg)),
            StashViewState::ShowingError => {
                // Any key leaves the error view.
                if matches!(msg, Msg::Key(_)) {
                    self.view_state = StashViewState::Ready;
                }
            }
            StashViewState::LoadingDocument => {}
        }

        Cmd::batch(cmds)
    }

    /// Updates for when the user is browsing the listing.
    fn handle_document_browsing(&mut self, common: &mut CommonModel, msg: &Msg) -> Cmd<Msg> {
        let mut cmds: Vec<Cmd<Msg>> = Vec::new();
        let num_docs = self.visible_count();

        if let Msg::Key(key) = msg {
            match key.as_str() {
                "k" | "ctrl+k" | "up" => self.move_cursor_up(),
                "j" | "ctrl+j" | "down" => self.move_cursor_down(),
                "home" | "g" => {
                    self.paginator_mut().page = 0;
                    self.set_cursor(0);
                }
                "end" | "G" => {
                    let last = self.paginator().total_pages.saturating_sub(1);
                    self.paginator_mut().page = last;
                    let items = self.paginator().items_on_page(num_docs) as i64;
                    self.set_cursor(items - 1);
                }
                super::keys::KEY_ESC => {
                    if self.filter_applied() {
                        self.reset_filtering(common);
                    }
                }
                "tab" | "L" => {
                    if !self.sections.is_empty() && self.filter_state != FilterState::Filtering {
                        self.section_index += 1;
                        if self.section_index >= self.sections.len() {
                            self.section_index = 0;
                        }
                        self.update_pagination(common);
                    }
                }
                "shift+tab" | "H" => {
                    if !self.sections.is_empty() && self.filter_state != FilterState::Filtering {
                        if self.section_index == 0 {
                            self.section_index = self.sections.len() - 1;
                        } else {
                            self.section_index -= 1;
                        }
                        self.update_pagination(common);
                    }
                }
                "F" => {
                    self.loaded = false;
                    return super::find_local_files(&common.cfg);
                }
                "e" => {
                    return match self.selected_markdown() {
                        Some(md) => super::editor::open_editor(&md.local_path, 0),
                        None => Cmd::None,
                    };
                }
                super::keys::KEY_ENTER => {
                    self.hide_status_message();
                    if num_docs != 0 {
                        if let Some(md) = self.selected_markdown().cloned() {
                            cmds.push(self.open_markdown(&md));
                        }
                    }
                }
                "/" => {
                    self.hide_status_message();
                    for md in &mut self.markdowns {
                        md.build_filter_value();
                    }
                    self.filtered_markdowns = (0..self.markdowns.len()).collect();
                    self.paginator_mut().page = 0;
                    self.set_cursor(0);
                    self.filter_state = FilterState::Filtering;
                    self.filter_input.cursor_end();
                    self.filter_input.focus();
                    return Cmd::tick(std::time::Duration::from_millis(530), Msg::Blink);
                }
                "?" => {
                    self.show_full_help = !self.show_full_help;
                    self.update_pagination(common);
                }
                "!" if self.err.is_some() && self.view_state == StashViewState::Ready => {
                    self.view_state = StashViewState::ShowingError;
                    return Cmd::None;
                }
                _ => {}
            }

            // The paginator's own key handling: glow leaves its key map empty,
            // so only these extra bindings page.
            match key.as_str() {
                "b" | "u" => self.paginator_mut().prev_page(),
                "f" | "d" => self.paginator_mut().next_page(),
                _ => {}
            }
        }

        // Keep the selection in bounds when paginating.
        let items_on_page = self.paginator().items_on_page(self.visible_count()) as i64;
        if self.cursor() > items_on_page - 1 {
            self.set_cursor((items_on_page - 1).max(0));
        }

        Cmd::batch(cmds)
    }

    /// Updates for when the user is editing the filter.
    fn handle_filtering(&mut self, common: &mut CommonModel, msg: &Msg) -> Cmd<Msg> {
        let mut cmds: Vec<Cmd<Msg>> = Vec::new();

        if let Msg::Key(key) = msg {
            match key.as_str() {
                super::keys::KEY_ESC => self.reset_filtering(common),
                super::keys::KEY_ENTER
                | "tab"
                | "shift+tab"
                | "ctrl+k"
                | "up"
                | "ctrl+j"
                | "down" => {
                    self.hide_status_message();
                    if !self.markdowns.is_empty() {
                        let visible = self.filtered_markdowns.clone();
                        if visible.is_empty() {
                            // Filtered down to nothing: drop the filter.
                            self.view_state = StashViewState::Ready;
                            self.reset_filtering(common);
                        } else if visible.len() == 1 {
                            // One result can just be opened.
                            self.view_state = StashViewState::Ready;
                            let md = self.markdowns[visible[0]].clone();
                            self.reset_filtering(common);
                            cmds.push(self.open_markdown(&md));
                        } else {
                            if self.sections.last().map(|s| s.key) != Some(SectionKey::Filter) {
                                self.sections.push(Section::new(SectionKey::Filter));
                                let styles = common.styles.clone();
                                let last = self.sections.len() - 1;
                                self.sections[last].paginator.active_dot =
                                    styles.bright_gray_fg.render("•");
                                self.sections[last].paginator.inactive_dot =
                                    styles.dark_gray_fg.render("•");
                            }
                            self.section_index = self.sections.len() - 1;
                            self.filter_input.blur();
                            self.filter_state = FilterState::FilterApplied;
                            if self.filter_input.value().is_empty() {
                                self.reset_filtering(common);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // The filter input sees every message, including the ones handled above.
        let current = self.filter_input.value();
        match msg {
            Msg::Key(key) => {
                let text = if key.chars().count() == 1 {
                    key.clone()
                } else {
                    String::new()
                };
                self.filter_input.update_key(key, &text);
            }
            Msg::Blink => {
                self.filter_input.blink();
                cmds.push(Cmd::tick(std::time::Duration::from_millis(530), Msg::Blink));
            }
            _ => {}
        }
        if self.filter_input.value() != current {
            cmds.push(filter_markdowns(self));
        }

        self.update_pagination(common);
        Cmd::batch(cmds)
    }
}

/// Reads a document's contents.
pub fn load_local_markdown(md: &Markdown) -> Cmd<Msg> {
    let mut md = md.clone();
    Cmd::Async(Box::new(move || {
        if md.local_path.is_empty() {
            return Some(Msg::Err("could not load file: missing path".into()));
        }
        match std::fs::read(&md.local_path) {
            Ok(data) => {
                md.body = String::from_utf8_lossy(&data).into_owned();
                Some(Msg::FetchedMarkdown(Box::new(md)))
            }
            Err(e) => Some(Msg::Err(format!(
                "open {}: {}",
                md.local_path,
                crate::deps::go_errno(&e)
            ))),
        }
    }))
}

/// Runs the filter over every document.
pub fn filter_markdowns(m: &StashModel) -> Cmd<Msg> {
    let query = m.filter_input.value();
    let applied = m.filter_applied();
    let count = m.markdowns.len();
    let targets: Vec<String> = m.markdowns.iter().map(|t| t.filter_value.clone()).collect();

    Cmd::Async(Box::new(move || {
        if query.is_empty() || !applied {
            // Everything, in order.
            return Some(Msg::FilteredMarkdown((0..count).collect()));
        }
        let ranks = fuzzy::find(&query, &targets);
        Some(Msg::FilteredMarkdown(
            ranks.into_iter().map(|r| r.index).collect(),
        ))
    }))
}

/// Highlights the parts of `haystack` that `needles` matched.
pub fn style_filtered_text(
    haystack: &str,
    needles: &str,
    default_style: &crate::deps::lipgloss::Style,
    matched_style: &crate::deps::lipgloss::Style,
) -> String {
    let normalized_hay = normalize(haystack);
    let matches = fuzzy::find(needles, &[normalized_hay]);
    if matches.is_empty() {
        return default_style.render(haystack);
    }

    let m = &matches[0];
    let mut b = String::new();
    for (i, c) in haystack.chars().enumerate() {
        // The original compares a rune index against the byte offsets the
        // matcher reports; they agree for ASCII, which is what this is for.
        let styled = m.matched_indexes.contains(&i);
        let style = if styled { matched_style } else { default_style };
        b.push_str(&style.render(&c.to_string()));
    }
    b
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

    fn stash(n: usize) -> (CommonModel, StashModel) {
        let c = common();
        let mut m = StashModel::new(&c.styles);
        let docs: Vec<Markdown> = (0..n)
            .map(|i| Markdown {
                note: format!("doc{i:02}.md"),
                local_path: format!("/tmp/doc{i:02}.md"),
                ..Markdown::default()
            })
            .collect();
        let mut c = c;
        m.add_markdowns(&c, docs);
        m.set_size(&mut c, 80, 24);
        (c, m)
    }

    #[test]
    fn per_page_comes_from_the_available_height() {
        let (_c, m) = stash(20);
        // (24 - 5 - 1 - 3) / 3
        assert_eq!(m.paginator().per_page, 5);
        assert_eq!(m.paginator().total_pages, 4);
    }

    #[test]
    fn at_least_one_item_fits_on_a_page() {
        let mut c = common();
        c.height = 1;
        let mut m = StashModel::new(&c.styles);
        m.set_size(&mut c, 80, 1);
        assert_eq!(m.paginator().per_page, 1);
    }

    #[test]
    fn the_cursor_pages_at_the_edges() {
        let (mut c, mut m) = stash(20);
        for _ in 0..5 {
            m.move_cursor_down();
        }
        assert_eq!(m.paginator().page, 1);
        assert_eq!(m.cursor(), 0);
        m.move_cursor_up();
        assert_eq!(m.paginator().page, 0);
        assert_eq!(m.cursor(), 4);
        m.set_cursor(0);
        m.move_cursor_up();
        assert_eq!(m.cursor(), 0, "the first row is the top");
        let _ = &mut c;
    }

    #[test]
    fn the_selected_document_follows_the_page_and_cursor() {
        let (_c, mut m) = stash(20);
        assert_eq!(m.selected_markdown().unwrap().note, "doc00.md");
        m.paginator_mut().page = 2;
        m.set_cursor(3);
        assert_eq!(m.selected_markdown().unwrap().note, "doc13.md");
        m.set_cursor(50);
        assert!(m.selected_markdown().is_none());
    }

    #[test]
    fn documents_are_sorted_as_they_arrive() {
        let c = common();
        let mut m = StashModel::new(&c.styles);
        m.add_markdowns(
            &c,
            vec![
                Markdown {
                    note: "b.md".into(),
                    ..Markdown::default()
                },
                Markdown {
                    note: "a.md".into(),
                    ..Markdown::default()
                },
            ],
        );
        let notes: Vec<&str> = m.markdowns.iter().map(|d| d.note.as_str()).collect();
        assert_eq!(notes, vec!["a.md", "b.md"]);
    }

    #[test]
    fn resetting_the_filter_drops_its_section() {
        let (c, mut m) = stash(5);
        m.sections.push(Section::new(SectionKey::Filter));
        m.section_index = 1;
        m.filter_state = FilterState::FilterApplied;
        m.reset_filtering(&c);
        assert_eq!(m.sections.len(), 1);
        assert_eq!(m.section_index, 0);
        assert_eq!(m.filter_state, FilterState::Unfiltered);
    }

    #[test]
    fn filtering_shows_the_filtered_set() {
        let (_c, mut m) = stash(5);
        m.filter_state = FilterState::Filtering;
        m.filtered_markdowns = vec![1, 3];
        let visible: Vec<&str> = m
            .visible_markdowns()
            .iter()
            .map(|d| d.note.as_str())
            .collect();
        assert_eq!(visible, vec!["doc01.md", "doc03.md"]);
    }

    #[test]
    fn matched_characters_are_styled_differently() {
        let plain = crate::deps::lipgloss::Style::new();
        let matched = crate::deps::lipgloss::Style::new().underline(true);
        let out = style_filtered_text("readme.md", "rm", &plain, &matched);
        assert!(out.contains("\u{1b}[4mr\u{1b}[m"), "{out}");
        assert!(out.contains("\u{1b}[4mm\u{1b}[m"), "{out}");
        assert_eq!(
            style_filtered_text("readme.md", "zzz", &plain, &matched),
            "readme.md",
            "a pattern that does not match leaves the text alone"
        );
    }
}
