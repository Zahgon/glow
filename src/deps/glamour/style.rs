//! The Glamour style model.
//!
//! Field names and JSON keys match `glamour/ansi`'s `StyleConfig` exactly, so
//! the upstream `styles/*.json` documents and any user-supplied stylesheet
//! deserialize unchanged.

use serde::{Deserialize, Serialize};

/// Colours and text attributes applied to a single element.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StylePrimitive {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub block_prefix: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub block_suffix: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prefix: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub suffix: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upper: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crossed_out: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conceal: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverse: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blink: Option<bool>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub format: String,
}

/// A [`StylePrimitive`] with block geometry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleBlock {
    #[serde(flatten)]
    pub primitive: StylePrimitive,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin: Option<u32>,
    /// Marks a block as a list, so nested lists can detect their depth. Never
    /// serialized: it is renderer state, not part of the stylesheet.
    #[serde(skip)]
    pub is_list: bool,
}

/// Checkbox markers for task list items.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleTask {
    #[serde(flatten)]
    pub primitive: StylePrimitive,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ticked: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unticked: String,
}

/// A code block, plus the optional syntax-highlighting theme.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleCodeBlock {
    #[serde(flatten)]
    pub block: StyleBlock,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub theme: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chroma: Option<Chroma>,
}

/// Per-token colours for syntax highlighting.
///
/// Deserialized so that user stylesheets round-trip, and so the `text` entry can
/// colour a code block; individual token classes are not resolved (see the
/// deliberate deviation recorded in `truth.md`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chroma {
    #[serde(default)]
    pub text: StylePrimitive,
    #[serde(default)]
    pub error: StylePrimitive,
    #[serde(default)]
    pub comment: StylePrimitive,
    #[serde(default)]
    pub comment_preproc: StylePrimitive,
    #[serde(default)]
    pub keyword: StylePrimitive,
    #[serde(default)]
    pub keyword_reserved: StylePrimitive,
    #[serde(default)]
    pub keyword_namespace: StylePrimitive,
    #[serde(default)]
    pub keyword_type: StylePrimitive,
    #[serde(default)]
    pub operator: StylePrimitive,
    #[serde(default)]
    pub punctuation: StylePrimitive,
    #[serde(default)]
    pub name: StylePrimitive,
    #[serde(default)]
    pub name_builtin: StylePrimitive,
    #[serde(default)]
    pub name_tag: StylePrimitive,
    #[serde(default)]
    pub name_attribute: StylePrimitive,
    #[serde(default)]
    pub name_class: StylePrimitive,
    #[serde(default)]
    pub name_constant: StylePrimitive,
    #[serde(default)]
    pub name_decorator: StylePrimitive,
    #[serde(default)]
    pub name_exception: StylePrimitive,
    #[serde(default)]
    pub name_function: StylePrimitive,
    #[serde(default)]
    pub name_other: StylePrimitive,
    #[serde(default)]
    pub literal: StylePrimitive,
    #[serde(default)]
    pub literal_number: StylePrimitive,
    #[serde(default)]
    pub literal_date: StylePrimitive,
    #[serde(default)]
    pub literal_string: StylePrimitive,
    #[serde(default)]
    pub literal_string_escape: StylePrimitive,
    #[serde(default)]
    pub generic_deleted: StylePrimitive,
    #[serde(default)]
    pub generic_emph: StylePrimitive,
    #[serde(default)]
    pub generic_inserted: StylePrimitive,
    #[serde(default)]
    pub generic_strong: StylePrimitive,
    #[serde(default)]
    pub generic_subheading: StylePrimitive,
    #[serde(default)]
    pub background: StylePrimitive,
}

/// A list, plus the extra indent applied to nested levels.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleList {
    #[serde(flatten)]
    pub block: StyleBlock,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub level_indent: u32,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// A table, plus its separator glyphs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleTable {
    #[serde(flatten)]
    pub block: StyleBlock,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center_separator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_separator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_separator: Option<String>,
}

/// The complete stylesheet consumed by the renderer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleConfig {
    #[serde(default)]
    pub document: StyleBlock,
    #[serde(default)]
    pub block_quote: StyleBlock,
    #[serde(default)]
    pub paragraph: StyleBlock,
    #[serde(default)]
    pub list: StyleList,
    #[serde(default)]
    pub heading: StyleBlock,
    #[serde(default)]
    pub h1: StyleBlock,
    #[serde(default)]
    pub h2: StyleBlock,
    #[serde(default)]
    pub h3: StyleBlock,
    #[serde(default)]
    pub h4: StyleBlock,
    #[serde(default)]
    pub h5: StyleBlock,
    #[serde(default)]
    pub h6: StyleBlock,
    #[serde(default)]
    pub text: StylePrimitive,
    #[serde(default)]
    pub strikethrough: StylePrimitive,
    #[serde(default)]
    pub emph: StylePrimitive,
    #[serde(default)]
    pub strong: StylePrimitive,
    #[serde(default, rename = "hr")]
    pub horizontal_rule: StylePrimitive,
    #[serde(default)]
    pub item: StylePrimitive,
    #[serde(default)]
    pub enumeration: StylePrimitive,
    #[serde(default)]
    pub task: StyleTask,
    #[serde(default)]
    pub link: StylePrimitive,
    #[serde(default)]
    pub link_text: StylePrimitive,
    #[serde(default)]
    pub image: StylePrimitive,
    #[serde(default)]
    pub image_text: StylePrimitive,
    #[serde(default)]
    pub code: StyleBlock,
    #[serde(default)]
    pub code_block: StyleCodeBlock,
    #[serde(default)]
    pub table: StyleTable,
    #[serde(default)]
    pub definition_list: StyleBlock,
    #[serde(default)]
    pub definition_term: StylePrimitive,
    #[serde(default)]
    pub definition_description: StylePrimitive,
    #[serde(default)]
    pub html_block: StyleBlock,
    #[serde(default)]
    pub html_span: StyleBlock,
}

/// Merges `child` onto `parent`; `child` wins wherever it sets a field.
///
/// `to_block` mirrors Glamour's flag of the same name: only in block context do
/// the prefix/suffix fields inherit from the parent.
pub fn cascade_primitive(
    parent: &StylePrimitive,
    child: &StylePrimitive,
    to_block: bool,
) -> StylePrimitive {
    let mut s = child.clone();

    s.color = parent.color.clone();
    s.background_color = parent.background_color.clone();
    s.underline = parent.underline;
    s.bold = parent.bold;
    s.upper = parent.upper;
    s.title = parent.title;
    s.lower = parent.lower;
    s.italic = parent.italic;
    s.crossed_out = parent.crossed_out;
    s.faint = parent.faint;
    s.conceal = parent.conceal;
    s.inverse = parent.inverse;
    s.blink = parent.blink;

    if to_block {
        s.block_prefix = parent.block_prefix.clone();
        s.block_suffix = parent.block_suffix.clone();
        s.prefix = parent.prefix.clone();
        s.suffix = parent.suffix.clone();
    }

    if child.color.is_some() {
        s.color = child.color.clone();
    }
    if child.background_color.is_some() {
        s.background_color = child.background_color.clone();
    }
    if child.underline.is_some() {
        s.underline = child.underline;
    }
    if child.bold.is_some() {
        s.bold = child.bold;
    }
    if child.upper.is_some() {
        s.upper = child.upper;
    }
    if child.lower.is_some() {
        s.lower = child.lower;
    }
    if child.title.is_some() {
        s.title = child.title;
    }
    if child.italic.is_some() {
        s.italic = child.italic;
    }
    if child.crossed_out.is_some() {
        s.crossed_out = child.crossed_out;
    }
    if child.faint.is_some() {
        s.faint = child.faint;
    }
    if child.conceal.is_some() {
        s.conceal = child.conceal;
    }
    if child.inverse.is_some() {
        s.inverse = child.inverse;
    }
    if child.blink.is_some() {
        s.blink = child.blink;
    }
    if !child.block_prefix.is_empty() {
        s.block_prefix = child.block_prefix.clone();
    }
    if !child.block_suffix.is_empty() {
        s.block_suffix = child.block_suffix.clone();
    }
    if !child.prefix.is_empty() {
        s.prefix = child.prefix.clone();
    }
    if !child.suffix.is_empty() {
        s.suffix = child.suffix.clone();
    }
    if !child.format.is_empty() {
        s.format = child.format.clone();
    }

    s
}

/// Block-level counterpart of [`cascade_primitive`].
pub fn cascade_block(parent: &StyleBlock, child: &StyleBlock, to_block: bool) -> StyleBlock {
    let mut s = child.clone();
    s.primitive = cascade_primitive(&parent.primitive, &child.primitive, to_block);

    if to_block {
        s.indent = parent.indent;
        s.margin = parent.margin;
    }
    if child.indent.is_some() {
        s.indent = child.indent;
    }
    s
}

/// Folds a chain of block styles left to right.
pub fn cascade_blocks(styles: &[&StyleBlock]) -> StyleBlock {
    let mut r = StyleBlock::default();
    for s in styles {
        r = cascade_block(&r, s, true);
    }
    r
}

/// Folds a chain of primitives left to right.
pub fn cascade_primitives(styles: &[&StylePrimitive]) -> StylePrimitive {
    let mut r = StylePrimitive::default();
    for s in styles {
        r = cascade_primitive(&r, s, true);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_overrides_parent_color() {
        let parent = StylePrimitive {
            color: Some("252".into()),
            bold: Some(true),
            ..Default::default()
        };
        let child = StylePrimitive {
            color: Some("39".into()),
            ..Default::default()
        };
        let out = cascade_primitive(&parent, &child, false);
        assert_eq!(out.color.as_deref(), Some("39"));
        assert_eq!(out.bold, Some(true));
    }

    #[test]
    fn prefixes_inherit_only_in_block_context() {
        let parent = StylePrimitive {
            prefix: "> ".into(),
            ..Default::default()
        };
        let child = StylePrimitive::default();
        assert_eq!(cascade_primitive(&parent, &child, false).prefix, "");
        assert_eq!(cascade_primitive(&parent, &child, true).prefix, "> ");
    }

    #[test]
    fn block_margin_inherits_only_in_block_context() {
        let parent = StyleBlock {
            margin: Some(2),
            ..Default::default()
        };
        let child = StyleBlock::default();
        assert_eq!(cascade_block(&parent, &child, false).margin, None);
        assert_eq!(cascade_block(&parent, &child, true).margin, Some(2));
    }
}
