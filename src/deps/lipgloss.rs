//! Terminal text styling.
//!
//! A narrow reimplementation of the `charm.land/lipgloss/v2` behaviour glow
//! actually uses: SGR colour/attribute rendering, word wrapping, left padding,
//! horizontal alignment, right margins and `MaxWidth` truncation. The SGR
//! parameter order matches `charmbracelet/x/ansi`'s `Style`, which is what
//! produces glow's on-screen bytes.

use crate::deps::ansi;

/// Ends a styled run. `x/ansi` writes the parameterless form, not `0m`.
pub const RESET: &str = "\u{1b}[m";

/// Terminal colour, as spelled in a Glamour style config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Color {
    /// One of the 16 basic ANSI colours.
    Basic(u8),
    /// An indexed 256-colour palette entry.
    Ansi256(u8),
    /// A 24-bit RGB colour.
    Rgb(u8, u8, u8),
}

impl Color {
    /// Parses a colour the way `lipgloss.Color` does: `"#rrggbb"`, `"#rgb"`, or
    /// a decimal palette index. Anything else has no colour.
    pub fn parse(s: &str) -> Option<Color> {
        let s = s.trim();
        if let Some(hex) = s.strip_prefix('#') {
            return match hex.len() {
                6 => Some(Color::Rgb(
                    u8::from_str_radix(&hex[0..2], 16).ok()?,
                    u8::from_str_radix(&hex[2..4], 16).ok()?,
                    u8::from_str_radix(&hex[4..6], 16).ok()?,
                )),
                3 => {
                    let d = |i: usize| -> Option<u8> {
                        let v = u8::from_str_radix(&hex[i..i + 1], 16).ok()?;
                        Some(v * 17)
                    };
                    Some(Color::Rgb(d(0)?, d(1)?, d(2)?))
                }
                _ => None,
            };
        }
        let n: u16 = s.parse().ok()?;
        if n > 255 {
            return None;
        }
        let n = n as u8;
        if n < 16 {
            Some(Color::Basic(n))
        } else {
            Some(Color::Ansi256(n))
        }
    }

    fn sgr(&self, background: bool) -> String {
        match *self {
            Color::Basic(n) => {
                let base = if background { 40 } else { 30 };
                let bright = if background { 100 } else { 90 };
                if n < 8 {
                    format!("{}", base + n as u16)
                } else {
                    format!("{}", bright + (n - 8) as u16)
                }
            }
            Color::Ansi256(n) => format!("{};5;{}", if background { 48 } else { 38 }, n),
            Color::Rgb(r, g, b) => {
                format!("{};2;{};{};{}", if background { 48 } else { 38 }, r, g, b)
            }
        }
    }
}

/// A set of SGR attributes, rendered in the order `charmbracelet/x/ansi` emits
/// them. Empty styles render their input unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sgr {
    params: Vec<String>,
}

impl Sgr {
    /// A style with no attributes.
    pub fn new() -> Self {
        Self::default()
    }
    /// True when no attribute has been set.
    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }
    /// Sets the foreground colour.
    pub fn foreground(mut self, c: &Color) -> Self {
        self.params.push(c.sgr(false));
        self
    }
    /// Sets the background colour.
    pub fn background(mut self, c: &Color) -> Self {
        self.params.push(c.sgr(true));
        self
    }
    /// Turns on underline.
    pub fn underline(mut self) -> Self {
        self.params.push("4".into());
        self
    }
    /// Turns on bold.
    pub fn bold(mut self) -> Self {
        self.params.push("1".into());
        self
    }
    /// Turns on italics.
    pub fn italic(mut self) -> Self {
        self.params.push("3".into());
        self
    }
    /// Turns on strikethrough.
    pub fn crossed_out(mut self) -> Self {
        self.params.push("9".into());
        self
    }
    /// Turns on faint.
    pub fn faint(mut self) -> Self {
        self.params.push("2".into());
        self
    }
    /// Turns on conceal.
    pub fn conceal(mut self) -> Self {
        self.params.push("8".into());
        self
    }
    /// Turns on reverse video.
    pub fn inverse(mut self) -> Self {
        self.params.push("7".into());
        self
    }
    /// Turns on blink.
    pub fn blink(mut self) -> Self {
        self.params.push("5".into());
        self
    }

    /// The escape sequence that enables this style.
    pub fn sequence(&self) -> String {
        if self.params.is_empty() {
            String::new()
        } else {
            format!("\u{1b}[{}m", self.params.join(";"))
        }
    }

    /// Wraps `s` in this style, or returns it untouched when the style is empty.
    pub fn styled(&self, s: &str) -> String {
        if self.params.is_empty() {
            return s.to_string();
        }
        format!("{}{s}{RESET}", self.sequence())
    }
}

/// A block style: colours plus width, padding, margins and truncation.
#[derive(Debug, Clone, Default)]
pub struct Style {
    value: String,
    fg: Option<Color>,
    bg: Option<Color>,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    reverse: bool,
    width: usize,
    height: usize,
    padding_left: usize,
    padding_right: usize,
    margin_right: usize,
    max_width: usize,
    has_props: bool,
}

impl Style {
    /// An empty style.
    pub fn new() -> Self {
        Self::default()
    }
    /// Stores a string rendered ahead of any argument, like `SetString`.
    pub fn set_string(mut self, s: &str) -> Self {
        self.value = s.to_string();
        self.has_props = true;
        self
    }
    /// The stored string.
    pub fn string(&self) -> String {
        self.clone().render("")
    }
    /// Sets the foreground colour.
    pub fn foreground(mut self, c: Color) -> Self {
        self.fg = Some(c);
        self.has_props = true;
        self
    }
    /// Sets the background colour.
    pub fn background(mut self, c: Color) -> Self {
        self.bg = Some(c);
        self.has_props = true;
        self
    }
    /// Toggles bold.
    pub fn bold(mut self, v: bool) -> Self {
        self.bold = v;
        self.has_props = true;
        self
    }
    /// Toggles italics.
    pub fn italic(mut self, v: bool) -> Self {
        self.italic = v;
        self.has_props = true;
        self
    }
    /// Toggles underline.
    pub fn underline(mut self, v: bool) -> Self {
        self.underline = v;
        self.has_props = true;
        self
    }
    /// Toggles strikethrough.
    pub fn strikethrough(mut self, v: bool) -> Self {
        self.strikethrough = v;
        self.has_props = true;
        self
    }
    /// Toggles reverse video.
    pub fn reverse(mut self, v: bool) -> Self {
        self.reverse = v;
        self.has_props = true;
        self
    }
    /// Sets the block width; lines are wrapped and padded to it.
    pub fn width(mut self, w: usize) -> Self {
        self.width = w;
        self.has_props = true;
        self
    }
    /// Sets the block height; short blocks gain blank lines.
    pub fn height(mut self, h: usize) -> Self {
        self.height = h;
        self.has_props = true;
        self
    }
    /// Sets padding, in the CSS order `top, right, bottom, left`.
    pub fn padding(mut self, _top: usize, right: usize, _bottom: usize, left: usize) -> Self {
        self.padding_left = left;
        self.padding_right = right;
        self.has_props = true;
        self
    }
    /// Sets the right margin.
    pub fn margin_right(mut self, n: usize) -> Self {
        self.margin_right = n;
        self.has_props = true;
        self
    }
    /// Truncates every rendered line to `n` columns.
    pub fn max_width(mut self, n: usize) -> Self {
        self.max_width = n;
        self.has_props = true;
        self
    }

    fn sgr(&self) -> Sgr {
        let mut te = Sgr::new();
        if self.bold {
            te = te.bold();
        }
        if self.italic {
            te = te.italic();
        }
        if self.underline {
            te = te.underline();
        }
        if self.reverse {
            te = te.inverse();
        }
        if let Some(fg) = &self.fg {
            te = te.foreground(fg);
        }
        if let Some(bg) = &self.bg {
            te = te.background(bg);
        }
        if self.strikethrough {
            te = te.crossed_out();
        }
        te
    }

    /// Renders `s` with this style.
    pub fn render(&self, s: &str) -> String {
        let mut str = if self.value.is_empty() {
            s.to_string()
        } else if s.is_empty() {
            self.value.clone()
        } else {
            format!("{} {}", self.value, s)
        };
        if !self.has_props {
            return str;
        }
        str = str.replace("\r\n", "\n");

        if self.width > 0 {
            let wrap_at = self.width as i64 - self.padding_left as i64 - self.padding_right as i64;
            str = ansi::wrap(&str, wrap_at, "");
        }

        let te = self.sgr();
        str = str
            .split('\n')
            .map(|l| te.styled(l))
            .collect::<Vec<_>>()
            .join("\n");

        // Whitespace carries only the background colour.
        let ws = match &self.bg {
            Some(bg) => Sgr::new().background(bg),
            None => Sgr::new(),
        };

        if self.padding_left > 0 {
            let pad = ws.styled(&" ".repeat(self.padding_left));
            str = str
                .split('\n')
                .map(|l| format!("{pad}{l}"))
                .collect::<Vec<_>>()
                .join("\n");
        }
        if self.padding_right > 0 {
            let pad = ws.styled(&" ".repeat(self.padding_right));
            str = str
                .split('\n')
                .map(|l| format!("{l}{pad}"))
                .collect::<Vec<_>>()
                .join("\n");
        }

        // Height is applied before horizontal alignment, so the blank lines it
        // adds are padded out to the width too.
        if self.height > 0 {
            let lines = str.split('\n').count();
            if lines < self.height {
                str.push_str(&"\n".repeat(self.height - lines));
            }
        }

        if str.contains('\n') || self.width != 0 {
            str = align_left(&str, self.width, &ws);
        }

        if self.margin_right > 0 {
            let pad = " ".repeat(self.margin_right);
            str = str
                .split('\n')
                .map(|l| format!("{l}{pad}"))
                .collect::<Vec<_>>()
                .join("\n");
        }

        if self.max_width > 0 {
            str = str
                .split('\n')
                .map(|l| ansi::truncate_with_tail(l, self.max_width, ""))
                .collect::<Vec<_>>()
                .join("\n");
        }

        str
    }
}

/// Pads every line out to `width` (or to the widest line when `width` is zero).
fn align_left(s: &str, width: usize, ws: &Sgr) -> String {
    let lines: Vec<&str> = s.split('\n').collect();
    let widest = lines
        .iter()
        .map(|l| ansi::string_width(l))
        .max()
        .unwrap_or(0);
    let target = width.max(widest);
    lines
        .iter()
        .map(|l| {
            let short = target.saturating_sub(ansi::string_width(l));
            if short == 0 {
                (*l).to_string()
            } else {
                format!("{}{}", l, ws.styled(&" ".repeat(short)))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Picks between a light and a dark variant, as `lipgloss.LightDark` does.
#[derive(Debug, Clone, Copy)]
pub struct LightDark(pub bool);

impl LightDark {
    /// Returns `dark` when the terminal background is dark, otherwise `light`.
    pub fn pick(&self, light: Color, dark: Color) -> Color {
        if self.0 {
            dark
        } else {
            light
        }
    }
}

/// The terminal pen: the SGR attributes and hyperlink currently in effect.
///
/// `lipgloss.WrapWriter` keeps this state so it can close and reopen a style
/// across a newline; reproducing it is what makes wrapped, coloured output
/// byte-identical to the original.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pen {
    fg: Option<Color>,
    bg: Option<Color>,
    underline_color: Option<Color>,
    underline: bool,
    bold: bool,
    faint: bool,
    italic: bool,
    blink: bool,
    rapid_blink: bool,
    reverse: bool,
    conceal: bool,
    strikethrough: bool,
    link: Option<(String, String)>,
}

impl Pen {
    /// Whether no attribute is set.
    pub fn is_zero(&self) -> bool {
        *self == Pen::default()
    }

    /// Whether a hyperlink is open.
    pub fn link_is_zero(&self) -> bool {
        self.link.is_none()
    }

    /// Folds one escape sequence into the pen.
    pub fn advance(&mut self, escape: &str) {
        if let Some(body) = escape
            .strip_prefix("\u{1b}[")
            .and_then(|b| b.strip_suffix('m'))
        {
            self.read_sgr(body);
        } else if let Some(body) = escape.strip_prefix("\u{1b}]8;") {
            let body = body.trim_end_matches('\u{7}').trim_end_matches("\u{1b}\\");
            let (params, url) = match body.split_once(';') {
                Some((p, u)) => (p, u),
                None => ("", body),
            };
            self.link = if url.is_empty() {
                None
            } else {
                Some((url.to_string(), params.to_string()))
            };
        }
    }

    fn read_sgr(&mut self, body: &str) {
        let parts: Vec<&str> = if body.is_empty() {
            vec!["0"]
        } else {
            body.split(';').collect()
        };
        let mut i = 0;
        while i < parts.len() {
            let n: u16 = parts[i].parse().unwrap_or(0);
            match n {
                0 => {
                    *self = Pen {
                        link: self.link.clone(),
                        ..Pen::default()
                    }
                }
                1 => self.bold = true,
                2 => self.faint = true,
                3 => self.italic = true,
                4 => self.underline = true,
                5 => self.blink = true,
                6 => self.rapid_blink = true,
                7 => self.reverse = true,
                8 => self.conceal = true,
                9 => self.strikethrough = true,
                21 | 24 => self.underline = false,
                22 => {
                    self.bold = false;
                    self.faint = false;
                }
                23 => self.italic = false,
                25 => {
                    self.blink = false;
                    self.rapid_blink = false;
                }
                27 => self.reverse = false,
                28 => self.conceal = false,
                29 => self.strikethrough = false,
                30..=37 => self.fg = Some(Color::Basic((n - 30) as u8)),
                39 => self.fg = None,
                40..=47 => self.bg = Some(Color::Basic((n - 40) as u8)),
                49 => self.bg = None,
                90..=97 => self.fg = Some(Color::Basic((n - 90 + 8) as u8)),
                100..=107 => self.bg = Some(Color::Basic((n - 100 + 8) as u8)),
                38 | 48 | 58 => {
                    let (color, consumed) = read_extended_color(&parts[i + 1..]);
                    match n {
                        38 => self.fg = color,
                        48 => self.bg = color,
                        _ => self.underline_color = color,
                    }
                    i += consumed;
                }
                59 => self.underline_color = None,
                _ => {}
            }
            i += 1;
        }
    }

    /// The SGR sequence that re-establishes this pen.
    ///
    /// The parameter order matches `ultraviolet`'s `Style.String`.
    pub fn sequence(&self) -> String {
        if self.is_zero_style() {
            return RESET.to_string();
        }
        let mut s = Sgr::new();
        if self.bold {
            s = s.bold();
        }
        if self.faint {
            s = s.faint();
        }
        if self.italic {
            s = s.italic();
        }
        if self.blink {
            s = s.blink();
        }
        if self.reverse {
            s = s.inverse();
        }
        if self.conceal {
            s = s.conceal();
        }
        if self.strikethrough {
            s = s.crossed_out();
        }
        if self.underline {
            s = s.underline();
        }
        if let Some(c) = &self.fg {
            s = s.foreground(c);
        }
        if let Some(c) = &self.bg {
            s = s.background(c);
        }
        s.sequence()
    }

    fn is_zero_style(&self) -> bool {
        Pen {
            link: None,
            ..self.clone()
        } == Pen::default()
    }
}

fn read_extended_color(parts: &[&str]) -> (Option<Color>, usize) {
    match parts.first().and_then(|p| p.parse::<u16>().ok()) {
        Some(5) => {
            let n = parts
                .get(1)
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(0);
            let c = if n < 16 {
                Color::Basic(n as u8)
            } else {
                Color::Ansi256(n as u8)
            };
            (Some(c), 2)
        }
        Some(2) => {
            let v = |i: usize| -> u8 {
                parts
                    .get(i)
                    .and_then(|p| p.parse::<u16>().ok())
                    .unwrap_or(0) as u8
            };
            (Some(Color::Rgb(v(1), v(2), v(3))), 4)
        }
        _ => (None, 1),
    }
}

/// Word-wraps `s`, then closes and reopens the pen around every newline.
///
/// This is `lipgloss.Wrap`: `ansi.Wrap` followed by a pass through a
/// `WrapWriter`. The writer's `Close` flush is deliberately not applied — in Go
/// it is deferred, so its output lands after the returned string is taken.
pub fn wrap(s: &str, width: i64, breakpoints: &str) -> String {
    let wrapped = ansi::wrap(s, width, breakpoints);
    let mut pen = Pen::default();
    let mut out = String::with_capacity(wrapped.len());
    for token in ansi::tokenize(&wrapped) {
        match token {
            ansi::Token::Escape(e) => {
                pen.advance(e);
                out.push_str(e);
            }
            ansi::Token::Text(t, _) => {
                if t == "\n" {
                    if !pen.is_zero_style() {
                        out.push_str(RESET);
                    }
                    if !pen.link_is_zero() {
                        out.push_str("\u{1b}]8;;\u{7}");
                    }
                    out.push('\n');
                    if let Some((url, params)) = pen.link.clone() {
                        out.push_str(&format!("\u{1b}]8;{params};{url}\u{7}"));
                    }
                    if !pen.is_zero_style() {
                        out.push_str(&pen.sequence());
                    }
                } else {
                    out.push_str(t);
                }
            }
        }
    }
    out
}

/// Whether the terminal background is dark.
///
/// Glow only consults this to choose between the `dark` and `light` Glamour
/// styles when rendering a pure code block with `--style auto`. Without a
/// queryable terminal the original falls back to dark, and so do we.
pub fn has_dark_background() -> bool {
    match std::env::var("COLORFGBG") {
        Ok(v) => v
            .rsplit(';')
            .next()
            .and_then(|bg| bg.trim().parse::<u8>().ok())
            .map(|bg| bg < 8)
            .unwrap_or(true),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_parses_hex_and_index() {
        assert_eq!(Color::parse("#04B575"), Some(Color::Rgb(4, 181, 117)));
        assert_eq!(Color::parse("252"), Some(Color::Ansi256(252)));
        assert_eq!(Color::parse("7"), Some(Color::Basic(7)));
        assert_eq!(Color::parse("nope"), None);
    }

    #[test]
    fn sgr_orders_params_like_x_ansi() {
        let s = Sgr::new()
            .foreground(&Color::Ansi256(228))
            .background(&Color::Ansi256(63))
            .bold();
        assert_eq!(s.styled("x"), "\u{1b}[38;5;228;48;5;63;1mx\u{1b}[m");
    }

    #[test]
    fn empty_sgr_is_transparent() {
        assert_eq!(Sgr::new().styled("x"), "x");
    }

    #[test]
    fn keyword_style_wraps_in_truecolor() {
        let s = Style::new().foreground(Color::parse("#04B575").unwrap());
        assert_eq!(
            s.render("with pizzazz"),
            "\u{1b}[38;2;4;181;117mwith pizzazz\u{1b}[m"
        );
    }

    #[test]
    fn paragraph_style_pads_every_line_to_width() {
        let s = Style::new().width(78).padding(0, 0, 0, 2);
        let out = s.render("\nRender markdown on the CLI, with pizzazz!");
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), 78);
        assert_eq!(ansi::string_width(lines[1]), 78);
        assert!(lines[1].starts_with("  Render markdown on the CLI,"));
    }
}
