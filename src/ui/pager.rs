//! The document pager: the viewport, its status bar and its help.

use std::path::Path;

use crate::deps::ansi;
use crate::deps::bubbles::viewport;
use crate::deps::bubbletea::Cmd;
use crate::deps::glamour::{self, render};
use crate::deps::lipgloss::Style;
use crate::deps::{clipboard, glamour::styles as gstyles};

use super::markdown::Markdown;
use super::stashitem::glow_logo_view;
use super::{indent, AppContext, CommonModel, Msg, ELLIPSIS};
use crate::utils;

/// Height of the status bar.
pub const STATUS_BAR_HEIGHT: usize = 1;
/// Width of the line-number gutter.
pub const LINE_NUMBER_WIDTH: usize = 4;

/// Whether the pager is browsing or showing a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PagerState {
    /// Reading the document.
    #[default]
    Browse,
    /// Showing an ephemeral message.
    StatusMessage,
}

/// The pager.
#[derive(Debug, Clone, Default)]
pub struct PagerModel {
    /// The scrolling viewport.
    pub viewport: viewport::Model,
    /// Browsing or showing a message.
    pub state: PagerState,
    /// Whether the help is expanded.
    pub show_help: bool,
    /// The ephemeral message.
    pub status_message: String,
    /// The document being read, before rendering, so it can be re-rendered on
    /// resize.
    pub current_document: Markdown,
    /// Cached height of the help view.
    help_height: usize,
}

impl PagerModel {
    /// A pager with an empty viewport.
    pub fn new() -> PagerModel {
        PagerModel {
            viewport: viewport::Model::new(),
            ..PagerModel::default()
        }
    }

    /// Resizes the viewport, allowing for the status bar and the help.
    pub fn set_size(&mut self, common: &CommonModel, w: usize, h: usize) {
        self.viewport.set_width(w);
        self.viewport
            .set_height(h.saturating_sub(STATUS_BAR_HEIGHT));

        if self.show_help {
            if self.help_height == 0 {
                self.help_height = self.help_view(common).matches('\n').count();
            }
            let height = self
                .viewport
                .height()
                .saturating_sub(STATUS_BAR_HEIGHT + self.help_height);
            self.viewport.set_height(height);
        }
    }

    /// Replaces the rendered content.
    pub fn set_content(&mut self, s: &str) {
        self.viewport.set_content(s);
    }

    /// Shows or hides the help, resizing around it.
    pub fn toggle_help(&mut self, common: &CommonModel) {
        self.show_help = !self.show_help;
        self.set_size(common, common.width, common.height);
        if self.viewport.past_bottom() {
            self.viewport.goto_bottom();
        }
    }

    /// Shows an ephemeral message.
    pub fn show_status_message(&mut self, message: &str) -> Cmd<Msg> {
        self.state = PagerState::StatusMessage;
        self.status_message = message.to_string();
        Cmd::tick(
            super::STATUS_MESSAGE_TIMEOUT,
            Msg::StatusMessageTimeout(AppContext::Pager),
        )
    }

    /// Clears the pager for the next document.
    pub fn unload(&mut self, common: &CommonModel) {
        if self.show_help {
            self.toggle_help(common);
        }
        self.state = PagerState::Browse;
        self.viewport.set_content("");
        self.viewport.set_y_offset(0);
    }

    /// Folds a message into the pager.
    pub fn update(&mut self, common: &CommonModel, msg: &Msg) -> Cmd<Msg> {
        let mut cmds: Vec<Cmd<Msg>> = Vec::new();

        match msg {
            Msg::Key(key) => match key.as_str() {
                "q" | super::keys::KEY_ESC => {
                    if self.state != PagerState::Browse {
                        self.state = PagerState::Browse;
                        return Cmd::None;
                    }
                }
                "home" | "g" => self.viewport.goto_top(),
                "end" | "G" => self.viewport.goto_bottom(),
                "d" => self.viewport.half_page_down(),
                "u" => self.viewport.half_page_up(),
                "e" => {
                    let total = self.viewport.total_line_count() as f64;
                    let mut lineno = round_to_even(total * self.viewport.scroll_percent());
                    if self.viewport.at_top() {
                        lineno = 0;
                    }
                    return super::editor::open_editor(&self.current_document.local_path, lineno);
                }
                "c" => {
                    // Copy through OSC 52 and through the system clipboard.
                    clipboard::osc52_copy(&self.current_document.body);
                    let _ = clipboard::write_all(&self.current_document.body);
                    cmds.push(self.show_status_message("Copied contents"));
                }
                "r" => return super::stash::load_local_markdown(&self.current_document),
                "?" => self.toggle_help(common),
                _ => {}
            },
            Msg::ContentRendered(s) => {
                self.set_content(s);
                cmds.push(Cmd::Async(Box::new(|| Some(Msg::WatchFile))));
            }
            Msg::Reload | Msg::EditorFinished(_) => {
                return super::stash::load_local_markdown(&self.current_document)
            }
            Msg::WindowSize { .. } => {
                return render_with_glamour(self, common, &self.current_document.body.clone())
            }
            Msg::StatusMessageTimeout(_) => self.state = PagerState::Browse,
            _ => {}
        }

        if let Msg::Key(key) = msg {
            self.viewport.update_key(key);
        }

        Cmd::batch(cmds)
    }

    /// Renders the viewport, the status bar and the help.
    pub fn view(&self, common: &CommonModel) -> String {
        let mut b = String::new();
        b.push_str(&self.viewport.view());
        b.push('\n');
        self.status_bar_view(common, &mut b);
        if self.show_help {
            b.push('\n');
            b.push_str(&self.help_view(common));
        }
        b
    }

    /// The one-line status bar.
    pub fn status_bar_view(&self, common: &CommonModel, b: &mut String) {
        let styles = &common.styles;
        let show_status_message = self.state == PagerState::StatusMessage;

        let logo = glow_logo_view(styles);

        let percent = self.viewport.scroll_percent().clamp(0.0, 1.0);
        let scroll_percent = format!(" {:>3.0}% ", percent * 100.0);
        let scroll_percent = if show_status_message {
            styles
                .status_bar_message_scroll_pos_style
                .render(&scroll_percent)
        } else {
            styles.status_bar_scroll_pos_style.render(&scroll_percent)
        };

        let help_note = if show_status_message {
            styles.status_bar_message_help_style.render(" ? Help ")
        } else {
            styles.status_bar_help_style.render(" ? Help ")
        };

        let note = if show_status_message {
            self.status_message.clone()
        } else {
            self.current_document.note.clone()
        };
        let room = common
            .width
            .saturating_sub(ansi::string_width(&logo))
            .saturating_sub(ansi::string_width(&scroll_percent))
            .saturating_sub(ansi::string_width(&help_note));
        let note = ansi::truncate_with_tail(&format!(" {note} "), room, ELLIPSIS);
        let note = if show_status_message {
            styles.status_bar_message_style.render(&note)
        } else {
            styles.status_bar_note_style.render(&note)
        };

        let padding = common
            .width
            .saturating_sub(ansi::string_width(&logo))
            .saturating_sub(ansi::string_width(&note))
            .saturating_sub(ansi::string_width(&scroll_percent))
            .saturating_sub(ansi::string_width(&help_note));
        let empty_space = " ".repeat(padding);
        let empty_space = if show_status_message {
            styles.status_bar_message_style.render(&empty_space)
        } else {
            styles.status_bar_note_style.render(&empty_space)
        };

        b.push_str(&format!(
            "{logo}{note}{empty_space}{scroll_percent}{help_note}"
        ));
    }

    /// The two-column key help.
    pub fn help_view(&self, common: &CommonModel) -> String {
        let col1 = [
            "g/home  go to top",
            "G/end   go to bottom",
            "c       copy contents",
            "e       edit this document",
            "r       reload this document",
            "esc     back to files",
            "q       quit",
        ];

        let mut s = String::from("\n");
        s.push_str(&format!("k/↑      up                  {}\n", col1[0]));
        s.push_str(&format!("j/↓      down                {}\n", col1[1]));
        s.push_str(&format!("b/pgup   page up             {}\n", col1[2]));
        s.push_str(&format!("f/pgdn   page down           {}\n", col1[3]));
        s.push_str(&format!("u        ½ page up           {}\n", col1[4]));
        s.push_str("d        ½ page down         ");
        s.push_str(col1[5]);

        let mut s = indent(&s, 2);

        // Fill the empty cells with spaces so the background colour reaches the
        // right edge.
        if common.width > 0 {
            s = s
                .split('\n')
                .map(|l| {
                    let n = common.width.saturating_sub(ansi::string_width(l));
                    format!("{l}{}", " ".repeat(n))
                })
                .collect::<Vec<_>>()
                .join("\n");
        }

        common.styles.help_view_style.render(&s)
    }

    /// The directory holding the current document.
    pub fn local_dir(&self) -> String {
        Path::new(&self.current_document.local_path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_string())
    }
}

/// `math.RoundToEven`, which Go uses to pick the line to open the editor at.
fn round_to_even(v: f64) -> u64 {
    if !v.is_finite() || v <= 0.0 {
        return 0;
    }
    let floor = v.floor();
    let diff = v - floor;
    #[allow(clippy::if_same_then_else)] // the tie case is a rule of its own
    let rounded = if diff > 0.5 {
        floor + 1.0
    } else if diff < 0.5 {
        floor
    } else if (floor as i64) % 2 == 0 {
        // A tie rounds to the even neighbour.
        floor
    } else {
        floor + 1.0
    };
    rounded as u64
}

/// Renders `md` through Glamour, off the main loop.
pub fn render_with_glamour(m: &PagerModel, common: &CommonModel, md: &str) -> Cmd<Msg> {
    let job = GlamourJob {
        markdown: md.to_string(),
        note: m.current_document.note.clone(),
        viewport_width: m.viewport.width(),
        cfg_max_width: common.cfg.glamour_max_width,
        cfg_style: common.cfg.glamour_style.clone(),
        preserve_new_lines: common.cfg.preserve_new_lines,
        show_line_numbers: common.cfg.show_line_numbers,
        glamour_enabled: common.cfg.glamour_enabled,
        line_number_style: common.styles.line_number_style.clone(),
    };
    Cmd::Async(Box::new(move || match job.render() {
        Ok(s) => Some(Msg::ContentRendered(s)),
        Err(e) => Some(Msg::Err(e)),
    }))
}

/// Everything the render needs, copied so it can run off the main loop.
struct GlamourJob {
    markdown: String,
    note: String,
    viewport_width: usize,
    cfg_max_width: u64,
    cfg_style: String,
    preserve_new_lines: bool,
    show_line_numbers: bool,
    glamour_enabled: bool,
    line_number_style: Style,
}

impl GlamourJob {
    /// This is where the magic happens.
    fn render(&self) -> Result<String, String> {
        if !self.glamour_enabled {
            return Ok(self.markdown.clone());
        }
        let trunc = Style::new().max_width(self.viewport_width.saturating_sub(LINE_NUMBER_WIDTH));

        let is_code = !utils::is_markdown_file(&self.note);
        let mut width = (self.cfg_max_width as usize).min(self.viewport_width);
        if is_code {
            width = 0;
        }

        let style = if self.cfg_style.is_empty() {
            gstyles::DARK_STYLE.to_string()
        } else {
            self.cfg_style.clone()
        };
        let styles = glamour::resolve_style(&utils::glamour_style(&style, is_code))
            .map_err(|e| format!("error creating glamour renderer: {e}"))?;

        let mut markdown = self.markdown.clone();
        if is_code {
            markdown = utils::wrap_code_block(&markdown, &utils::extension(&self.note));
        }

        let mut out = render::Renderer::new(render::Options {
            word_wrap: width as i64,
            preserve_new_lines: self.preserve_new_lines,
            styles,
            ..Default::default()
        })
        .render(&markdown);

        if is_code {
            out = out.trim().to_string();
        }

        let lines: Vec<&str> = out.split('\n').collect();
        let mut content = String::new();
        for (i, s) in lines.iter().enumerate() {
            if is_code || self.show_line_numbers {
                content.push_str(&self.line_number_style.render(&format!(
                    "{:>width$}",
                    i + 1,
                    width = LINE_NUMBER_WIDTH
                )));
                content.push_str(&trunc.render(s));
            } else {
                content.push_str(s);
            }
            if i + 1 < lines.len() {
                content.push('\n');
            }
        }
        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::styles::Styles;
    use crate::ui::Config;

    fn common() -> CommonModel {
        CommonModel {
            cfg: Config {
                glamour_style: "notty".into(),
                glamour_max_width: 80,
                ..Config::default()
            },
            cwd: String::new(),
            width: 40,
            height: 10,
            styles: Styles::new(true),
        }
    }

    fn pager(common: &CommonModel) -> PagerModel {
        let mut m = PagerModel::new();
        m.set_size(common, common.width, common.height);
        m
    }

    #[test]
    fn the_status_bar_is_exactly_one_line_wide_enough() {
        let c = common();
        let mut m = pager(&c);
        m.current_document.note = "readme.md".into();
        let mut b = String::new();
        m.status_bar_view(&c, &mut b);
        assert!(!b.contains('\n'));
        assert_eq!(ansi::string_width(&b), c.width);
        assert!(b.contains("readme.md"), "{b}");
        assert!(b.contains("? Help"), "{b}");
    }

    #[test]
    fn a_long_note_is_truncated_with_an_ellipsis() {
        let c = common();
        let mut m = pager(&c);
        m.current_document.note = "a-very-long-document-name-that-will-not-fit.md".into();
        let mut b = String::new();
        m.status_bar_view(&c, &mut b);
        assert!(b.contains(ELLIPSIS), "{b}");
        assert_eq!(ansi::string_width(&b), c.width);
    }

    #[test]
    fn the_scroll_position_is_a_percentage() {
        let c = common();
        let mut m = pager(&c);
        m.set_content(
            &(0..100)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let mut b = String::new();
        m.status_bar_view(&c, &mut b);
        assert!(b.contains("  0%"), "{b}");
        m.viewport.goto_bottom();
        let mut b = String::new();
        m.status_bar_view(&c, &mut b);
        assert!(b.contains("100%"), "{b}");
    }

    #[test]
    fn the_help_view_covers_both_columns() {
        let c = common();
        let m = pager(&c);
        let help = m.help_view(&c);
        assert!(help.contains("½ page down"), "{help}");
        assert!(help.contains("back to files"), "{help}");
        assert_eq!(help.matches('\n').count(), 7);
    }

    #[test]
    fn showing_the_help_shrinks_the_viewport() {
        let c = common();
        let mut m = pager(&c);
        let before = m.viewport.height();
        m.toggle_help(&c);
        assert!(m.viewport.height() < before);
        m.toggle_help(&c);
        assert_eq!(m.viewport.height(), before);
    }

    #[test]
    fn round_to_even_breaks_ties_towards_the_even_number() {
        assert_eq!(round_to_even(0.5), 0);
        assert_eq!(round_to_even(1.5), 2);
        assert_eq!(round_to_even(2.5), 2);
        assert_eq!(round_to_even(2.6), 3);
        assert_eq!(round_to_even(-1.0), 0);
    }

    #[test]
    fn line_numbers_are_right_aligned_in_four_columns() {
        let job = GlamourJob {
            markdown: "x = 1\ny = 2\n".into(),
            note: "code.rs".into(),
            viewport_width: 40,
            cfg_max_width: 80,
            cfg_style: "notty".into(),
            preserve_new_lines: false,
            show_line_numbers: false,
            glamour_enabled: true,
            line_number_style: Style::new(),
        };
        let out = job.render().expect("renders");
        assert!(out.starts_with("   1"), "{out:?}");
        assert!(out.contains("   2"), "{out:?}");
    }

    #[test]
    fn glamour_can_be_turned_off_entirely() {
        let job = GlamourJob {
            markdown: "# raw\n".into(),
            note: "a.md".into(),
            viewport_width: 40,
            cfg_max_width: 80,
            cfg_style: "notty".into(),
            preserve_new_lines: false,
            show_line_numbers: false,
            glamour_enabled: false,
            line_number_style: Style::new(),
        };
        assert_eq!(job.render().expect("renders"), "# raw\n");
    }
}
