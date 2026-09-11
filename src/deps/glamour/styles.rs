//! The built-in Glamour stylesheets.
//!
//! The JSON documents are the upstream `glamour/styles/*.json` files, embedded
//! verbatim so the styles carry over byte for byte rather than being
//! transcribed by hand.

use super::style::StyleConfig;

/// Style name: plain ASCII, no colour.
pub const ASCII_STYLE: &str = "ascii";
/// Style name: the default dark theme.
pub const DARK_STYLE: &str = "dark";
/// Style name: Dracula.
pub const DRACULA_STYLE: &str = "dracula";
/// Style name: Tokyo Night.
pub const TOKYO_NIGHT_STYLE: &str = "tokyo-night";
/// Style name: the default light theme.
pub const LIGHT_STYLE: &str = "light";
/// Style name: no terminal styling at all.
pub const NOTTY_STYLE: &str = "notty";
/// Style name: pink.
pub const PINK_STYLE: &str = "pink";

const ASCII_JSON: &str = include_str!("styles/ascii.json");
const DARK_JSON: &str = include_str!("styles/dark.json");
const DRACULA_JSON: &str = include_str!("styles/dracula.json");
const LIGHT_JSON: &str = include_str!("styles/light.json");
const NOTTY_JSON: &str = include_str!("styles/notty.json");
const PINK_JSON: &str = include_str!("styles/pink.json");
const TOKYO_NIGHT_JSON: &str = include_str!("styles/tokyo-night.json");

/// The names of every built-in style, in registry order.
pub const DEFAULT_STYLE_NAMES: &[&str] = &[
    ASCII_STYLE,
    DARK_STYLE,
    LIGHT_STYLE,
    NOTTY_STYLE,
    PINK_STYLE,
    DRACULA_STYLE,
    TOKYO_NIGHT_STYLE,
];

/// Returns the built-in style with this name, if there is one.
pub fn default_style(name: &str) -> Option<StyleConfig> {
    let json = match name {
        ASCII_STYLE => ASCII_JSON,
        DARK_STYLE => DARK_JSON,
        DRACULA_STYLE => DRACULA_JSON,
        LIGHT_STYLE => LIGHT_JSON,
        NOTTY_STYLE => NOTTY_JSON,
        PINK_STYLE => PINK_JSON,
        TOKYO_NIGHT_STYLE => TOKYO_NIGHT_JSON,
        _ => return None,
    };
    serde_json::from_str(json).ok()
}

/// Whether `name` is one of the built-in styles.
pub fn is_default_style(name: &str) -> bool {
    DEFAULT_STYLE_NAMES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_style_parses() {
        for name in DEFAULT_STYLE_NAMES {
            assert!(default_style(name).is_some(), "{name} failed to parse");
        }
    }

    #[test]
    fn unknown_style_is_none() {
        assert!(default_style("nonexistent-style").is_none());
        assert!(!is_default_style("nonexistent-style"));
    }

    #[test]
    fn notty_carries_upstream_values() {
        let s = default_style(NOTTY_STYLE).unwrap();
        assert_eq!(s.document.margin, Some(2));
        assert_eq!(s.document.primitive.block_prefix, "\n");
        assert_eq!(s.document.primitive.block_suffix, "\n");
        assert_eq!(s.block_quote.indent, Some(1));
        assert_eq!(s.block_quote.indent_token.as_deref(), Some("| "));
        assert_eq!(s.list.level_indent, 4);
        assert_eq!(s.h2.primitive.prefix, "## ");
        assert_eq!(s.item.block_prefix, "• ");
        assert_eq!(s.enumeration.block_prefix, ". ");
        assert_eq!(s.task.ticked, "[x] ");
        assert_eq!(s.task.unticked, "[ ] ");
        assert_eq!(s.horizontal_rule.format, "\n--------\n");
        assert_eq!(s.emph.block_prefix, "*");
        assert_eq!(s.strong.block_prefix, "**");
        assert_eq!(s.strikethrough.block_prefix, "~~");
        assert_eq!(s.image_text.format, "Image: {{.text}} →");
        assert_eq!(s.table.center_separator.as_deref(), Some("|"));
        assert_eq!(s.table.row_separator.as_deref(), Some("-"));
    }

    #[test]
    fn dark_carries_upstream_colors() {
        let s = default_style(DARK_STYLE).unwrap();
        assert_eq!(s.document.primitive.color.as_deref(), Some("252"));
        assert_eq!(s.heading.primitive.color.as_deref(), Some("39"));
        assert_eq!(s.heading.primitive.bold, Some(true));
        assert_eq!(s.h1.primitive.background_color.as_deref(), Some("63"));
        assert_eq!(s.h6.primitive.bold, Some(false));
        assert_eq!(s.block_quote.indent_token.as_deref(), Some("│ "));
        assert_eq!(s.list.level_indent, 2);
        assert_eq!(s.task.ticked, "[✓] ");
        assert!(s.code_block.chroma.is_some());
        assert_eq!(s.definition_description.block_prefix, "\n🠶 ");
    }
}
