//! Every style the TUI uses, resolved for the terminal's background.

use crate::deps::lipgloss::{Color, LightDark, Style};

/// The styles the TUI draws with.
///
/// They are resolved from light and dark variants according to the terminal's
/// background colour, which the runtime reports once at start-up. Until then
/// the dark variants are used.
#[derive(Debug, Clone)]
pub struct Styles {
    light_dark: LightDark,

    /// The fuchsia used for selections and the logo background.
    pub fuchsia: Color,

    /// Dimmed body text.
    pub dim_normal_fg: Style,
    /// Bright grey.
    pub bright_gray_fg: Style,
    /// Dimmed bright grey.
    pub dim_bright_gray_fg: Style,
    /// Grey.
    pub gray_fg: Style,
    /// Mid grey.
    pub mid_gray_fg: Style,
    /// Dark grey.
    pub dark_gray_fg: Style,
    /// Green.
    pub green_fg: Style,
    /// Semi-dimmed green.
    pub semi_dim_green_fg: Style,
    /// Dimmed green.
    pub dim_green_fg: Style,
    /// Fuchsia.
    pub fuchsia_fg: Style,
    /// Dimmed fuchsia.
    pub dim_fuchsia_fg: Style,
    /// Dull fuchsia.
    pub dull_fuchsia_fg: Style,
    /// Dimmed dull fuchsia.
    pub dim_dull_fuchsia_fg: Style,
    /// Red.
    pub red_fg: Style,

    /// An unselected section tab.
    pub tab_style: Style,
    /// The selected section tab.
    pub selected_tab_style: Style,
    /// The `ERROR` badge.
    pub error_title_style: Style,
    /// Subtle text.
    pub subtle_style: Style,
    /// The pagination indicator.
    pub pagination_style: Style,

    /// The scroll position in the status bar.
    pub status_bar_scroll_pos_style: Style,
    /// The note in the status bar.
    pub status_bar_note_style: Style,
    /// The help hint in the status bar.
    pub status_bar_help_style: Style,
    /// A status message in the status bar.
    pub status_bar_message_style: Style,
    /// The scroll position while a status message shows.
    pub status_bar_message_scroll_pos_style: Style,
    /// The help hint while a status message shows.
    pub status_bar_message_help_style: Style,
    /// The expanded help view.
    pub help_view_style: Style,
    /// Line numbers in the pager.
    pub line_number_style: Style,

    /// The ` • ` divider.
    pub divider_dot: Style,
    /// The ` │ ` divider.
    pub divider_bar: Style,
    /// The ` Glow ` logo.
    pub logo_style: Style,
    /// The loading spinner.
    pub stash_spinner_style: Style,
    /// The filter input's prompt.
    pub stash_input_prompt_style: Style,
}

fn color(s: &str) -> Color {
    Color::parse(s).expect("a literal colour parses")
}

impl Styles {
    /// Builds every style for the given terminal background.
    pub fn new(is_dark: bool) -> Styles {
        let ld = LightDark(is_dark);
        let adaptive = |light: &str, dark: &str| ld.pick(color(light), color(dark));

        // Colors
        let normal_dim = adaptive("#A49FA5", "#777777");
        let gray = adaptive("#909090", "#626262");
        let mid_gray = adaptive("#B2B2B2", "#4A4A4A");
        let dark_gray = adaptive("#DDDADA", "#3C3C3C");
        let bright_gray = adaptive("#847A85", "#979797");
        let dim_bright_gray = adaptive("#C2B8C2", "#4D4D4D");
        let cream = adaptive("#FFFDF5", "#FFFDF5");
        let yellow_green = adaptive("#04B575", "#ECFD65");
        let fuchsia = adaptive("#EE6FF8", "#EE6FF8");
        let dim_fuchsia = adaptive("#F1A8FF", "#99519E");
        let dull_fuchsia = adaptive("#F793FF", "#AD58B4");
        let dim_dull_fuchsia = adaptive("#F6C9FF", "#7B4380");
        let green = color("#04B575");
        let red = adaptive("#FF4672", "#ED567A");
        let semi_dim_green = adaptive("#35D79C", "#036B46");
        let dim_green = adaptive("#72D2B0", "#0B5137");

        // Pager colors
        let mint_green = adaptive("#89F0CB", "#89F0CB");
        let dark_green = adaptive("#1C8760", "#1C8760");
        let line_number_fg = adaptive("#656565", "#7D7D7D");
        let status_bar_note_fg = adaptive("#656565", "#7D7D7D");
        let status_bar_bg = adaptive("#E6E6E6", "#242424");

        let fg = |c: Color| Style::new().foreground(c);

        Styles {
            light_dark: ld,
            fuchsia: fuchsia.clone(),

            dim_normal_fg: fg(normal_dim),
            bright_gray_fg: fg(bright_gray),
            dim_bright_gray_fg: fg(dim_bright_gray),
            gray_fg: fg(gray.clone()),
            mid_gray_fg: fg(mid_gray),
            dark_gray_fg: fg(dark_gray),
            green_fg: fg(green.clone()),
            semi_dim_green_fg: fg(semi_dim_green),
            dim_green_fg: fg(dim_green),
            fuchsia_fg: fg(fuchsia.clone()),
            dim_fuchsia_fg: fg(dim_fuchsia),
            dull_fuchsia_fg: fg(dull_fuchsia),
            dim_dull_fuchsia_fg: fg(dim_dull_fuchsia),
            red_fg: fg(red.clone()),

            tab_style: fg(adaptive("#909090", "#626262")),
            selected_tab_style: fg(adaptive("#333333", "#979797")),
            error_title_style: Style::new()
                .foreground(cream)
                .background(red)
                .padding(0, 1, 0, 1),
            subtle_style: fg(adaptive("#9B9B9B", "#5C5C5C")),
            pagination_style: fg(adaptive("#9B9B9B", "#5C5C5C")),

            status_bar_scroll_pos_style: Style::new()
                .foreground(adaptive("#949494", "#5A5A5A"))
                .background(status_bar_bg.clone()),
            status_bar_note_style: Style::new()
                .foreground(status_bar_note_fg.clone())
                .background(status_bar_bg),
            status_bar_help_style: Style::new()
                .foreground(status_bar_note_fg.clone())
                .background(adaptive("#DCDCDC", "#323232")),
            status_bar_message_style: Style::new()
                .foreground(mint_green.clone())
                .background(dark_green.clone()),
            status_bar_message_scroll_pos_style: Style::new()
                .foreground(mint_green)
                .background(dark_green),
            status_bar_message_help_style: Style::new()
                .foreground(color("#B6FFE4"))
                .background(green),
            help_view_style: Style::new()
                .foreground(status_bar_note_fg)
                .background(adaptive("#f2f2f2", "#1B1B1B")),
            line_number_style: fg(line_number_fg),

            divider_dot: fg(dark_gray_of(ld)).set_string(" • "),
            divider_bar: fg(dark_gray_of(ld)).set_string(" │ "),
            logo_style: Style::new()
                .foreground(color("#ECFD65"))
                .background(fuchsia)
                .bold(true),
            stash_spinner_style: fg(gray),
            stash_input_prompt_style: fg(yellow_green).margin_right(1),
        }
    }

    /// Picks between a light and a dark variant for this background.
    pub fn adaptive(&self, light: &str, dark: &str) -> Color {
        self.light_dark.pick(color(light), color(dark))
    }
}

fn dark_gray_of(ld: LightDark) -> Color {
    ld.pick(color("#DDDADA"), color("#3C3C3C"))
}

impl Default for Styles {
    fn default() -> Styles {
        Styles::new(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes recorded from `charm.land/lipgloss/v2` itself: it emits the bold
    /// attribute before the colours, which is the order glow's chrome carries.
    #[test]
    fn the_logo_is_yellow_on_fuchsia() {
        let s = Styles::new(true);
        assert_eq!(
            s.logo_style.render(" Glow "),
            "\u{1b}[1;38;2;236;253;101;48;2;238;111;248m Glow \u{1b}[m"
        );
    }

    #[test]
    fn dividers_carry_their_string() {
        let s = Styles::new(true);
        assert!(s.divider_dot.string().contains(" • "));
        assert!(s.divider_bar.string().contains(" │ "));
    }

    #[test]
    fn the_background_picks_the_variant() {
        let dark = Styles::new(true);
        let light = Styles::new(false);
        assert_eq!(
            dark.adaptive("#A49FA5", "#777777"),
            Color::parse("#777777").unwrap()
        );
        assert_eq!(
            light.adaptive("#A49FA5", "#777777"),
            Color::parse("#A49FA5").unwrap()
        );
    }
}
