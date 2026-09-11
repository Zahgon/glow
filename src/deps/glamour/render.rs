//! The ANSI renderer.
//!
//! A port of `glamour/ansi`: a stack of block buffers, each of which collects
//! its children, word-wraps them to the width left over after indentation and
//! margins, then re-emits them indented and right-padded. Element styling
//! cascades down the stack exactly as it does upstream.

use super::ast::{self, Alignment, Node};
use super::style::{
    cascade_block, cascade_blocks, cascade_primitives, StyleBlock, StyleConfig, StylePrimitive,
};
use super::table::{Align, Border, Table};
use crate::deps::ansi;
use crate::deps::lipgloss::{self, Color, Sgr};

/// Renderer options.
#[derive(Debug, Clone)]
pub struct Options {
    /// Base URL relative links resolve against.
    pub base_url: String,
    /// Column at which text wraps; `0` disables wrapping and padding.
    pub word_wrap: i64,
    /// Whether over-long table cells wrap rather than truncate.
    pub table_wrap: bool,
    /// Whether links inside tables render inline instead of as footnotes.
    pub inline_table_links: bool,
    /// Whether paragraphs keep their source line breaks.
    pub preserve_new_lines: bool,
    /// The stylesheet.
    pub styles: StyleConfig,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            base_url: String::new(),
            word_wrap: 80,
            table_wrap: true,
            inline_table_links: false,
            preserve_new_lines: false,
            styles: StyleConfig::default(),
        }
    }
}

/// One frame of the block stack.
struct Block {
    buf: String,
    style: StyleBlock,
}

/// Where a write goes: the final output, the current block, or its parent.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Out,
    Current,
    Parent,
}

/// A link collected out of a table for the footnote list below it.
#[derive(Clone, PartialEq, Eq)]
struct TableLink {
    href: String,
    content: String,
    kind: LinkKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LinkKind {
    Auto,
    Image,
    Regular,
}

/// Renders a markdown document to ANSI text.
pub struct Renderer {
    opts: Options,
    stack: Vec<Block>,
    out: String,
    scratch: String,
    table_links: Vec<TableLink>,
    table_images: Vec<TableLink>,
    in_table: bool,
}

impl Renderer {
    /// Creates a renderer with the given options.
    pub fn new(opts: Options) -> Self {
        Renderer {
            opts,
            stack: Vec::new(),
            out: String::new(),
            scratch: String::new(),
            table_links: Vec::new(),
            table_images: Vec::new(),
            in_table: false,
        }
    }

    /// Renders `source` and returns the styled text.
    pub fn render(mut self, source: &str) -> String {
        let doc = ast::parse(source);
        self.document(&doc);
        self.out
    }

    // ---- block stack -------------------------------------------------------

    fn indent(&self) -> u32 {
        self.stack.iter().filter_map(|b| b.style.indent).sum()
    }

    fn margin(&self) -> u32 {
        self.stack.iter().filter_map(|b| b.style.margin).sum()
    }

    /// Columns available to the current block's content.
    fn width(&self) -> u32 {
        let used = self.indent() + self.margin() * 2;
        if self.opts.word_wrap <= 0 || used as i64 > self.opts.word_wrap {
            return 0;
        }
        self.opts.word_wrap as u32 - used
    }

    fn current_style(&self) -> StyleBlock {
        self.stack
            .last()
            .map(|b| b.style.clone())
            .unwrap_or_default()
    }

    fn parent_style(&self) -> StyleBlock {
        if self.stack.len() < 2 {
            StyleBlock::default()
        } else {
            self.stack[self.stack.len() - 2].style.clone()
        }
    }

    /// The style a child primitive inherits from the current block.
    fn with(&self, child: &StylePrimitive) -> StylePrimitive {
        let sb = StyleBlock {
            primitive: child.clone(),
            ..Default::default()
        };
        cascade_block(&self.current_style(), &sb, false).primitive
    }

    fn write(&mut self, target: Target, s: &str) {
        if s.is_empty() {
            return;
        }
        match target {
            Target::Out => self.out.push_str(s),
            Target::Current => match self.stack.last_mut() {
                Some(b) => b.buf.push_str(s),
                None => self.out.push_str(s),
            },
            Target::Parent => {
                let n = self.stack.len();
                if n >= 2 {
                    self.stack[n - 2].buf.push_str(s);
                } else {
                    self.scratch.push_str(s);
                }
            }
        }
    }

    /// Where a node's own output goes while it is being entered.
    fn entering_target(&self) -> Target {
        if self.stack.is_empty() {
            Target::Out
        } else {
            Target::Current
        }
    }

    /// Where a node's output goes as it is finished.
    fn exiting_target(&self) -> Target {
        if self.stack.is_empty() {
            Target::Out
        } else {
            Target::Parent
        }
    }

    // ---- primitives --------------------------------------------------------

    /// Applies a style primitive to `s` and returns the styled text.
    fn style_text(rules: &StylePrimitive, s: &str) -> String {
        if s.is_empty() {
            return String::new();
        }
        let mut s = s.to_string();
        if rules.upper == Some(true) {
            s = s.to_uppercase();
        }
        if rules.lower == Some(true) {
            s = s.to_lowercase();
        }
        if rules.title == Some(true) {
            s = title_case(&s);
        }

        let mut style = Sgr::new();
        if let Some(c) = rules.color.as_deref().and_then(Color::parse) {
            style = style.foreground(&c);
        }
        if let Some(c) = rules.background_color.as_deref().and_then(Color::parse) {
            style = style.background(&c);
        }
        if rules.underline == Some(true) {
            style = style.underline();
        }
        if rules.bold == Some(true) {
            style = style.bold();
        }
        if rules.italic == Some(true) {
            style = style.italic();
        }
        if rules.crossed_out == Some(true) {
            style = style.crossed_out();
        }
        if rules.inverse == Some(true) {
            style = style.inverse();
        }
        if rules.blink == Some(true) {
            style = style.blink();
        }
        style.styled(&s)
    }

    fn render_text(&mut self, target: Target, rules: &StylePrimitive, s: &str) {
        let styled = Self::style_text(rules, s);
        self.write(target, &styled);
    }

    /// Renders a leaf element: prefixes, the token, and suffixes.
    ///
    /// `st1` is the enclosing block's style and `st2` the element's own.
    fn base_element(
        &mut self,
        target: Target,
        st1: &StylePrimitive,
        st2: &StylePrimitive,
        prefix: &str,
        suffix: &str,
        token: &str,
    ) {
        self.render_text(target, st1, prefix);
        self.render_text(target, st1, &st2.block_prefix);
        self.render_text(target, st2, &st2.prefix);

        let body = if st2.format.is_empty() {
            token.to_string()
        } else {
            format_token(&st2.format, token)
        };
        self.render_text(target, st2, &unescape_markdown(&body));

        self.render_text(target, st2, &st2.suffix);
        self.render_text(target, st1, &st2.block_suffix);
        self.render_text(target, st1, suffix);
    }

    /// Writes `content` through the indent/margin/padding pipeline.
    ///
    /// Mirrors Glamour's `MarginWriter`: `indent + margin` leading tokens per
    /// line, then every completed line right-padded to the block width.
    fn margin_write(&mut self, target: Target, content: &str, rules: &StyleBlock) {
        let indent = rules.indent.unwrap_or(0) + rules.margin.unwrap_or(0);
        let indent_token = rules.indent_token.clone().unwrap_or_else(|| " ".into());
        let padding = self.width() as usize;
        let pad_style = rules.primitive.clone();
        let indent_style = self.parent_style().primitive;

        let mut chunk = String::new();
        let mut line = String::new();
        let mut at_line_start = true;

        for token in ansi::tokenize(content) {
            match token {
                ansi::Token::Escape(e) => {
                    if at_line_start {
                        for _ in 0..indent {
                            chunk.push_str(&Self::style_text(&indent_style, &indent_token));
                        }
                        at_line_start = false;
                    }
                    line.push_str(e);
                    chunk.push_str(e);
                }
                ansi::Token::Text(t, _) => {
                    if at_line_start {
                        for _ in 0..indent {
                            chunk.push_str(&Self::style_text(&indent_style, &indent_token));
                        }
                        at_line_start = false;
                    }
                    if t == "\n" {
                        let w = ansi::string_width(&line);
                        if padding > 0 && w < padding {
                            for _ in 0..(padding - w) {
                                chunk.push_str(&Self::style_text(&pad_style, " "));
                            }
                        }
                        line.clear();
                        at_line_start = true;
                    } else {
                        line.push_str(t);
                    }
                    chunk.push_str(t);
                }
            }
        }
        self.write(target, &chunk);
    }

    /// Writes `content` with only the indent applied, as code blocks and tables do.
    fn indent_write(&mut self, target: Target, content: &str, indent: u32, style: &StylePrimitive) {
        if content.is_empty() {
            return;
        }
        let mut chunk = String::new();
        let mut at_line_start = true;
        for token in ansi::tokenize(content) {
            let s = match token {
                ansi::Token::Escape(e) => e,
                ansi::Token::Text(t, _) => t,
            };
            if at_line_start {
                for _ in 0..indent {
                    chunk.push_str(&Self::style_text(style, " "));
                }
                at_line_start = false;
            }
            if s == "\n" {
                at_line_start = true;
            }
            chunk.push_str(s);
        }
        self.write(target, &chunk);
    }

    // ---- blocks ------------------------------------------------------------

    fn document(&mut self, node: &Node) {
        let style = self.opts.styles.document.clone();
        // Enter.
        self.render_text(
            Target::Out,
            &StylePrimitive::default(),
            &style.primitive.block_prefix,
        );
        self.stack.push(Block {
            buf: String::new(),
            style: style.clone(),
        });
        let prefix = style.primitive.prefix.clone();
        let cur = self.current_style().primitive;
        self.render_text(Target::Current, &cur, &prefix);

        self.children(node.children());

        // Finish.
        let content = self.stack.last().map(|b| b.buf.clone()).unwrap_or_default();
        let w = self.width() as i64;
        let wrapped = lipgloss::wrap(&content, w, " ,.;-+|");
        let rules = self.current_style();
        self.margin_write(Target::Out, &wrapped, &rules);

        let cur = self.current_style().primitive;
        self.render_text(Target::Out, &cur, &style.primitive.suffix);
        self.render_text(
            Target::Out,
            &StylePrimitive::default(),
            &style.primitive.block_suffix,
        );
        self.stack.pop();
    }

    /// Renders a run of sibling nodes.
    fn children(&mut self, nodes: &[Node]) {
        for (i, node) in nodes.iter().enumerate() {
            self.node(node, i == 0);
        }
    }

    fn node(&mut self, node: &Node, first: bool) {
        match node {
            Node::Document(children) => self.children(children),
            Node::Heading { level, children } => self.heading(*level, children, first),
            Node::Paragraph(children) => self.paragraph(children, first),
            Node::BlockQuote(children) => self.block_quote(children),
            Node::List {
                ordered,
                start,
                items,
            } => self.list(*ordered, *start, items),
            Node::ListItem { .. } => {}
            Node::CodeBlock { code, .. } => self.code_block(code),
            Node::HtmlBlock(html) => {
                let st1 = self.current_style().primitive;
                let st2 = self.with(&self.opts.styles.html_block.primitive.clone());
                let token = sanitize_html(html, true);
                let t = self.entering_target();
                self.base_element(t, &st1, &st2, "", "", &token);
            }
            Node::RawHtml(html) => {
                let st1 = self.current_style().primitive;
                let st2 = self.with(&self.opts.styles.html_span.primitive.clone());
                let token = sanitize_html(html, true);
                let t = self.entering_target();
                self.base_element(t, &st1, &st2, "", "", &token);
            }
            Node::ThematicBreak => {
                let st1 = self.current_style().primitive;
                let st2 = self.with(&self.opts.styles.horizontal_rule.clone());
                let t = self.entering_target();
                self.base_element(t, &st1, &st2, "", "", "");
            }
            Node::Table {
                alignments,
                head,
                rows,
            } => self.table(alignments, head, rows),
            Node::DefinitionList(children) => self.definition_list(children),
            Node::DefinitionTerm(children) => {
                let t = self.entering_target();
                self.write(t, "\n");
                let st1 = self.current_style().primitive;
                let st2 = self.with(&self.opts.styles.definition_term.clone());
                self.base_element(t, &st1, &st2, "", "", "");
                self.children(children);
            }
            Node::DefinitionDescription(children) => {
                let t = self.entering_target();
                let st1 = self.current_style().primitive;
                let st2 = self.with(&self.opts.styles.definition_description.clone());
                self.base_element(t, &st1, &st2, "", "", "");
                self.children(children);
                self.write(Target::Current, "\n");
            }
            Node::Text(text) => {
                let st1 = self.current_style().primitive;
                let st2 = self.with(&self.opts.styles.text.clone());
                let t = self.entering_target();
                self.base_element(t, &st1, &st2, "", "", text);
            }
            Node::CodeSpan(text) => {
                let style =
                    cascade_block(&self.current_style(), &self.opts.styles.code.clone(), false)
                        .primitive;
                let token = format!("{}{}{}", style.prefix, text, style.suffix);
                let t = self.entering_target();
                self.render_text(t, &style, &token);
            }
            Node::Emphasis { level, children } => {
                let style = if *level > 1 {
                    self.opts.styles.strong.clone()
                } else {
                    self.opts.styles.emph.clone()
                };
                self.emphasis(children, &style);
            }
            Node::Strikethrough { text, .. } => {
                let st1 = self.current_style().primitive;
                let st2 = self.with(&self.opts.styles.strikethrough.clone());
                let t = self.entering_target();
                self.base_element(t, &st1, &st2, "", "", text);
            }
            Node::Link {
                destination,
                children,
            } => self.link(destination, children),
            Node::AutoLink { url, is_email } => self.auto_link(url, *is_email),
            Node::Image { text, destination } => self.image(text, destination),
        }
    }

    fn heading(&mut self, level: usize, children: &[Node], first: bool) {
        let base = self.opts.styles.heading.clone();
        let lvl = match level {
            1 => self.opts.styles.h1.clone(),
            2 => self.opts.styles.h2.clone(),
            3 => self.opts.styles.h3.clone(),
            4 => self.opts.styles.h4.clone(),
            5 => self.opts.styles.h5.clone(),
            _ => self.opts.styles.h6.clone(),
        };
        let rules = cascade_blocks(&[&base, &lvl]);

        let t = self.entering_target();
        if !first {
            let cur = self.current_style().primitive;
            self.render_text(t, &cur, "\n");
        }

        let block_style = cascade_block(&self.current_style(), &rules, false);
        self.stack.push(Block {
            buf: String::new(),
            style: block_style,
        });

        let parent = self.parent_style().primitive;
        self.write(
            Target::Parent,
            &Self::style_text(&parent, &rules.primitive.block_prefix),
        );
        let cur = self.current_style().primitive;
        self.render_text(Target::Current, &cur, &rules.primitive.prefix);

        self.children(children);

        let content = self.stack.last().map(|b| b.buf.clone()).unwrap_or_default();
        let w = self.width() as i64;
        let flow = lipgloss::wrap(&content, w, "");
        let block_rules = self.current_style();
        let target = self.exiting_target();
        self.margin_write(target, &flow, &block_rules);

        let cur = self.current_style().primitive;
        self.render_text(target, &cur, &block_rules.primitive.suffix);
        let parent = self.parent_style().primitive;
        self.render_text(target, &parent, &block_rules.primitive.block_suffix);
        self.stack.pop();
    }

    fn paragraph(&mut self, children: &[Node], first: bool) {
        let rules = self.opts.styles.paragraph.clone();
        let t = self.entering_target();
        if !first {
            self.write(t, "\n");
        }
        let block_style = cascade_block(&self.current_style(), &rules, false);
        self.stack.push(Block {
            buf: String::new(),
            style: block_style,
        });

        let parent = self.parent_style().primitive;
        self.write(
            Target::Parent,
            &Self::style_text(&parent, &rules.primitive.block_prefix),
        );
        let cur = self.current_style().primitive;
        self.render_text(Target::Current, &cur, &rules.primitive.prefix);

        self.children(children);

        let content = self.stack.last().map(|b| b.buf.clone()).unwrap_or_default();
        let block_rules = self.current_style();
        let target = self.exiting_target();
        if !content.trim().is_empty() {
            let blk = if self.opts.preserve_new_lines {
                content.clone()
            } else {
                content.replace('\n', " ")
            };
            let w = self.width() as i64;
            let flow = lipgloss::wrap(&blk, w, "");
            let flow = format!("{flow}\n");
            self.margin_write(target, &flow, &block_rules);
        }

        let cur = self.current_style().primitive;
        self.render_text(target, &cur, &block_rules.primitive.suffix);
        let parent = self.parent_style().primitive;
        self.render_text(target, &parent, &block_rules.primitive.block_suffix);
        self.stack.pop();
    }

    fn block_quote(&mut self, children: &[Node]) {
        let t = self.entering_target();
        self.write(t, "\n");
        let rules = cascade_block(
            &self.current_style(),
            &self.opts.styles.block_quote.clone(),
            false,
        );
        self.block(children, rules, false);
    }

    fn definition_list(&mut self, children: &[Node]) {
        let rules = cascade_block(
            &self.current_style(),
            &self.opts.styles.definition_list.clone(),
            false,
        );
        self.block(children, rules, true);
    }

    fn list(&mut self, ordered: bool, start: u64, items: &[Node]) {
        let t = self.entering_target();
        self.write(t, "\n");

        let mut s = self.opts.styles.list.block.clone();
        if s.indent.is_none() {
            s.indent = Some(0);
        }
        // A nested list indents by the configured level indent.
        if self.stack.iter().any(|b| b.style.is_list) {
            s.indent = Some(self.opts.styles.list.level_indent);
        }
        let rules = cascade_block(&self.current_style(), &s, false);

        let block_style = StyleBlock {
            is_list: true,
            ..rules.clone()
        };
        self.stack.push(Block {
            buf: String::new(),
            style: block_style,
        });

        let parent = self.parent_style().primitive;
        self.write(
            Target::Parent,
            &Self::style_text(&parent, &rules.primitive.block_prefix),
        );
        let cur = self.current_style().primitive;
        self.render_text(Target::Current, &cur, &rules.primitive.prefix);

        for (i, item) in items.iter().enumerate() {
            if let Node::ListItem { children, task } = item {
                self.list_item(children, *task, ordered, start, i, i + 1 == items.len());
            }
        }

        self.finish_block(rules, true);
    }

    #[allow(clippy::too_many_arguments)]
    fn list_item(
        &mut self,
        children: &[Node],
        task: Option<bool>,
        ordered: bool,
        start: u64,
        index: usize,
        last: bool,
    ) {
        let st1 = self.current_style().primitive;
        let t = self.entering_target();

        match task {
            Some(checked) => {
                let task_style = self.opts.styles.task.clone();
                let pre = if checked {
                    task_style.ticked.clone()
                } else {
                    task_style.unticked.clone()
                };
                let st2 = self.with(&task_style.primitive);
                self.base_element(t, &st1, &st2, &pre, "", "");
            }
            None if ordered => {
                let mut e = index as u64 + 1;
                if start != 1 {
                    e += start - 1;
                }
                let st2 = self.with(&self.opts.styles.enumeration.clone());
                self.base_element(t, &st1, &st2, &e.to_string(), "", "");
            }
            None => {
                let st2 = self.with(&self.opts.styles.item.clone());
                self.base_element(t, &st1, &st2, "", "", "");
            }
        }

        self.children(children);

        // A trailing newline separates items, except after a nested list or
        // after the final item.
        let ends_with_list = matches!(children.last(), Some(Node::List { .. }));
        if !ends_with_list && !last {
            self.write(Target::Current, "\n");
        }
    }

    /// Pushes a margin block, renders `children` into it and finishes it.
    fn block(&mut self, children: &[Node], rules: StyleBlock, newline: bool) {
        self.stack.push(Block {
            buf: String::new(),
            style: rules.clone(),
        });
        let parent = self.parent_style().primitive;
        self.write(
            Target::Parent,
            &Self::style_text(&parent, &rules.primitive.block_prefix),
        );
        let cur = self.current_style().primitive;
        self.render_text(Target::Current, &cur, &rules.primitive.prefix);

        self.children(children);
        self.finish_block(rules, newline);
    }

    fn finish_block(&mut self, rules: StyleBlock, newline: bool) {
        let content = self.stack.last().map(|b| b.buf.clone()).unwrap_or_default();
        let w = self.width() as i64;
        let mut wrapped = lipgloss::wrap(&content, w, " ,.;-+|");
        if newline {
            wrapped.push('\n');
        }
        let block_rules = self.current_style();
        let target = self.exiting_target();
        self.margin_write(target, &wrapped, &block_rules);

        let cur = self.current_style().primitive;
        self.render_text(target, &cur, &rules.primitive.suffix);
        let parent = self.parent_style().primitive;
        self.render_text(target, &parent, &rules.primitive.block_suffix);
        self.stack.pop();
    }

    fn code_block(&mut self, code: &str) {
        let t = self.entering_target();
        self.write(t, "\n");

        let rules = self.opts.styles.code_block.clone();
        let indent = rules.block.indent.unwrap_or(0) + rules.block.margin.unwrap_or(0);
        let cur = self.current_style().primitive;

        let st1 = cur.clone();
        let st2 = self.with(&rules.block.primitive);
        let mut buf = Renderer::new(self.opts.clone());
        buf.stack.push(Block {
            buf: String::new(),
            style: self.current_style(),
        });
        buf.base_element(Target::Out, &st1, &st2, "", "", code);
        let body = buf.out;

        self.indent_write(t, &body, indent, &cur);
    }

    fn emphasis(&mut self, children: &[Node], style: &StylePrimitive) {
        let t = self.entering_target();
        for child in children {
            self.override_render(t, child, style);
        }
    }

    /// Whether Glamour renders this node through an element that accepts a
    /// style override — `BaseElement` or `EmphasisElement`.
    fn is_overridable(node: &Node) -> bool {
        matches!(
            node,
            Node::Text(_)
                | Node::Strikethrough { .. }
                | Node::ThematicBreak
                | Node::HtmlBlock(_)
                | Node::RawHtml(_)
                | Node::Emphasis { .. }
        )
    }

    /// Renders `node` with `over` cascaded onto its own style.
    fn override_render(&mut self, target: Target, node: &Node, over: &StylePrimitive) {
        match node {
            Node::Emphasis { level, children } => {
                let base = if *level > 1 {
                    self.opts.styles.strong.clone()
                } else {
                    self.opts.styles.emph.clone()
                };
                let merged = cascade_primitives(&[&base, over]);
                let t = self.entering_target();
                for child in children {
                    self.override_render(t, child, &merged);
                }
            }
            Node::Text(text) => {
                self.base_override(target, &self.opts.styles.text.clone(), over, text)
            }
            Node::Strikethrough { text, .. } => {
                self.base_override(target, &self.opts.styles.strikethrough.clone(), over, text)
            }
            Node::HtmlBlock(html) => {
                let token = sanitize_html(html, true);
                self.base_override(
                    target,
                    &self.opts.styles.html_block.primitive.clone(),
                    over,
                    &token,
                )
            }
            Node::RawHtml(html) => {
                let token = sanitize_html(html, true);
                self.base_override(
                    target,
                    &self.opts.styles.html_span.primitive.clone(),
                    over,
                    &token,
                )
            }
            Node::ThematicBreak => {
                self.base_override(target, &self.opts.styles.horizontal_rule.clone(), over, "")
            }
            other => self.node(other, false),
        }
    }

    fn base_override(
        &mut self,
        target: Target,
        own: &StylePrimitive,
        over: &StylePrimitive,
        token: &str,
    ) {
        let cur = self.current_style().primitive;
        let st1 = cascade_primitives(&[&cur, over]);
        let with_own = self.with(own);
        let st2 = cascade_primitives(&[&with_own, over]);
        self.base_element(target, &st1, &st2, "", "", token);
    }

    /// Renders `node` into a detached buffer with the current block's style.
    fn render_detached(&mut self, node: &Node, over: Option<&StylePrimitive>) -> String {
        let mut sub = Renderer::new(self.opts.clone());
        sub.in_table = self.in_table;
        sub.table_links = self.table_links.clone();
        sub.table_images = self.table_images.clone();
        sub.stack.push(Block {
            buf: String::new(),
            style: self.current_style(),
        });
        match over {
            Some(o) => sub.override_render(Target::Current, node, o),
            None => sub.node(node, false),
        }
        sub.stack.pop().map(|b| b.buf).unwrap_or_default()
    }

    fn link(&mut self, destination: &str, children: &[Node]) {
        if self.in_table && !self.opts.inline_table_links {
            let content: String = children.iter().map(|c| c.text_content()).collect();
            let tl = TableLink {
                href: destination.to_string(),
                content: content.clone(),
                kind: LinkKind::Regular,
            };
            let text = link_with_suffix(&tl, &self.table_links);
            let (open, close, _) = make_hyperlink(destination);
            let st1 = self.current_style().primitive;
            let st2 = self.with(&self.opts.styles.link_text.clone());
            let t = self.entering_target();
            let token = format!("{open}{text}{close}");
            self.base_element(t, &st1, &st2, "", "", &token);
            return;
        }

        let (open, close, valid) = make_hyperlink(destination);
        // Text part. An element that accepts a style override is styled first
        // and only then wrapped in the hyperlink; anything else is rendered
        // plain and the whole token is styled as link text.
        let link_text = self.opts.styles.link_text.clone();
        for child in children {
            let t = self.entering_target();
            if Self::is_overridable(child) {
                let inner = self.render_detached(child, Some(&link_text));
                self.write(t, &format!("{open}{inner}{close}"));
            } else {
                let inner = self.render_detached(child, None);
                let token = format!("{open}{inner}{close}");
                let st1 = self.current_style().primitive;
                let st2 = self.with(&link_text);
                self.base_element(t, &st1, &st2, "", "", &token);
            }
        }
        // Href part.
        if valid {
            let token = format!(
                "{open}{}{close}",
                resolve_relative_url(&self.opts.base_url, destination)
            );
            let st1 = self.current_style().primitive;
            let st2 = self.with(&self.opts.styles.link.clone());
            let t = self.entering_target();
            self.base_element(t, &st1, &st2, " ", "", &token);
        }
    }

    fn auto_link(&mut self, url: &str, is_email: bool) {
        let mut u = url.to_string();
        if is_email && !u.to_lowercase().starts_with("mailto:") {
            u = format!("mailto:{u}");
        }

        if self.in_table && !self.opts.inline_table_links {
            let mut content = link_domain(&u);
            if let Some(short) = super::autolink::detect(&u) {
                content = short;
            }
            let tl = TableLink {
                href: u.clone(),
                content,
                kind: LinkKind::Auto,
            };
            let text = link_with_suffix(&tl, &self.table_links);
            let (open, close, _) = make_hyperlink(&u);
            let token = format!("{open}{text}{close}");
            let st1 = self.current_style().primitive;
            let st2 = self.with(&self.opts.styles.link_text.clone());
            let t = self.entering_target();
            self.base_element(t, &st1, &st2, "", "", &token);
            return;
        }

        let (open, close, valid) = make_hyperlink(&u);
        if is_email {
            // Email autolinks show the address and hide the `mailto:` href.
            let token = format!("{open}{url}{close}");
            let st1 = self.current_style().primitive;
            let st2 = self.with(&self.opts.styles.link_text.clone());
            let t = self.entering_target();
            self.base_element(t, &st1, &st2, "", "", &token);
        } else if valid {
            let token = format!(
                "{open}{}{close}",
                resolve_relative_url(&self.opts.base_url, &u)
            );
            let st1 = self.current_style().primitive;
            let st2 = self.with(&self.opts.styles.link.clone());
            let t = self.entering_target();
            self.base_element(t, &st1, &st2, "", "", &token);
        }
    }

    fn image(&mut self, text: &str, destination: &str) {
        let text_only = self.in_table && !self.opts.inline_table_links;
        let mut text = text.to_string();
        if text_only {
            if text.is_empty() {
                text = link_domain(destination);
            }
            let tl = TableLink {
                href: destination.to_string(),
                content: text.clone(),
                kind: LinkKind::Image,
            };
            text = link_with_suffix(&tl, &self.table_images);
        }

        let (open, close, _) = make_hyperlink(destination);
        let mut style = self.opts.styles.image_text.clone();
        if text_only {
            style.format = style
                .format
                .strip_suffix(" →")
                .map(str::to_string)
                .unwrap_or(style.format);
        }

        if !text.is_empty() {
            let token = format!("{open}{text}{close}");
            let st1 = self.current_style().primitive;
            let st2 = self.with(&style);
            let t = self.entering_target();
            self.base_element(t, &st1, &st2, "", "", &token);
        }
        if text_only {
            return;
        }
        if !destination.is_empty() {
            let token = format!(
                "{open}{}{close}",
                resolve_relative_url(&self.opts.base_url, destination)
            );
            let st1 = self.current_style().primitive;
            let st2 = self.with(&self.opts.styles.image.clone());
            let t = self.entering_target();
            self.base_element(t, &st1, &st2, " ", "", &token);
        }
    }

    fn table(&mut self, alignments: &[Alignment], head: &[Vec<Node>], rows: &[Vec<Vec<Node>>]) {
        let t = self.entering_target();
        self.write(t, "\n");

        let rules = self.opts.styles.table.clone();
        let indent = rules.block.indent.unwrap_or(0) + rules.block.margin.unwrap_or(0);
        let width = self.width() as usize;

        // Collect the footnote links before rendering the cells, so a cell can
        // reference its own footnote number.
        self.table_links.clear();
        self.table_images.clear();
        if !self.opts.inline_table_links {
            let mut links = Vec::new();
            let mut images = Vec::new();
            for cell in head.iter().chain(rows.iter().flatten()) {
                for node in cell {
                    collect_links(node, &mut links, &mut images);
                }
            }
            self.table_links = dedup(links);
            self.table_images = dedup(images);
        }

        self.in_table = true;
        let cell_style = rules.block.primitive.clone();
        let headers: Vec<String> = head
            .iter()
            .map(|c| self.render_cell(c, &cell_style))
            .collect();
        let body: Vec<Vec<String>> = rows
            .iter()
            .map(|r| r.iter().map(|c| self.render_cell(c, &cell_style)).collect())
            .collect();
        self.in_table = false;

        let mut table = Table::new(width);
        table.wrap = self.opts.table_wrap;
        table.headers = headers;
        table.rows = body;
        table.aligns = alignments
            .iter()
            .map(|a| match a {
                Alignment::Right => Align::Right,
                Alignment::Center => Align::Center,
                _ => Align::Left,
            })
            .collect();
        if let (Some(row), Some(col), Some(center)) = (
            rules.row_separator.clone(),
            rules.column_separator.clone(),
            rules.center_separator.clone(),
        ) {
            table.border = Border {
                top: row,
                left: col,
                middle: center,
            };
        }
        table.margin = rules.block.margin.map(|m| m as usize).unwrap_or(1);

        let rendered = table.render();
        let cur = self.current_style().primitive;
        self.indent_write(Target::Current, &rendered, indent, &cur);

        let st2 = self.with(&rules.block.primitive);
        self.render_text(Target::Current, &st2, &rules.block.primitive.suffix);
        let cur = self.current_style().primitive;
        self.render_text(Target::Current, &cur, &rules.block.primitive.block_suffix);

        self.print_table_links();
        self.write(Target::Current, "\n");
    }

    fn render_cell(&mut self, cell: &[Node], style: &StylePrimitive) -> String {
        let mut sub = Renderer::new(self.opts.clone());
        sub.in_table = true;
        sub.table_links = self.table_links.clone();
        sub.table_images = self.table_images.clone();
        sub.stack.push(Block {
            buf: String::new(),
            style: self.current_style(),
        });
        for node in cell {
            if Self::is_overridable(node) {
                sub.override_render(Target::Current, node, style);
            } else {
                let inner = sub.render_detached(node, None);
                let st1 = sub.current_style().primitive;
                let st2 = sub.with(style);
                sub.base_element(Target::Current, &st1, &st2, "", "", &inner);
            }
        }
        sub.stack.pop().map(|b| b.buf).unwrap_or_default()
    }

    fn print_table_links(&mut self) {
        if self.opts.inline_table_links
            || (self.table_links.is_empty() && self.table_images.is_empty())
        {
            return;
        }
        let term_width = self.width() as usize;
        let cur = self.current_style().primitive;

        let render_list = |renderer: &mut Self, list: &[TableLink]| {
            for (i, item) in list.iter().enumerate() {
                let position = i + 1;
                let padding = digits(list.len()).saturating_sub(digits(position));
                renderer.render_text(Target::Current, &cur, "\n");

                let mut style;
                let token;
                match item.kind {
                    LinkKind::Image => {
                        style = renderer.opts.styles.image_text.clone();
                        style.prefix = format!("[{position}]: {}", style.prefix);
                        token = format!("{}{}", " ".repeat(padding), item.content);
                    }
                    _ => {
                        style = renderer.opts.styles.link_text.clone();
                        token = format!("{}[{position}]: {}", " ".repeat(padding), item.content);
                    }
                }
                let st1 = renderer.current_style().primitive;
                let st2 = renderer.with(&style);
                let before = renderer.stack.last().map(|b| b.buf.len()).unwrap_or(0);
                renderer.base_element(Target::Current, &st1, &st2, "", "", &token);
                let link_text = renderer
                    .stack
                    .last()
                    .map(|b| b.buf[before..].to_string())
                    .unwrap_or_default();

                renderer.render_text(Target::Current, &cur, " ");

                let (open, close, _) = make_hyperlink(&item.href);
                let max = term_width
                    .saturating_sub(ansi::string_width(&link_text))
                    .saturating_sub(1);
                let href = ansi::truncate_with_tail(&item.href, max, "…");
                let href_style = match item.kind {
                    LinkKind::Image => renderer.opts.styles.image.clone(),
                    _ => renderer.opts.styles.link.clone(),
                };
                let st1 = renderer.current_style().primitive;
                let st2 = renderer.with(&href_style);
                let token = format!("{open}{href}{close}");
                renderer.base_element(Target::Current, &st1, &st2, "", "", &token);
            }
        };

        let links = self.table_links.clone();
        let images = self.table_images.clone();
        if !links.is_empty() {
            self.render_text(Target::Current, &cur, "\n");
        }
        render_list(self, &links);
        if !images.is_empty() {
            self.render_text(Target::Current, &cur, "\n");
        }
        render_list(self, &images);
    }
}

fn digits(n: usize) -> usize {
    n.to_string().len()
}

fn dedup(list: Vec<TableLink>) -> Vec<TableLink> {
    let mut out: Vec<TableLink> = Vec::new();
    for item in list {
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

fn link_with_suffix(tl: &TableLink, list: &[TableLink]) -> String {
    match list.iter().position(|x| x == tl) {
        Some(i) => format!("{}[{}]", tl.content, i + 1),
        None => tl.content.clone(),
    }
}

fn collect_links(node: &Node, links: &mut Vec<TableLink>, images: &mut Vec<TableLink>) {
    match node {
        Node::AutoLink { url, is_email } => {
            let u = if *is_email && !url.to_lowercase().starts_with("mailto:") {
                format!("mailto:{url}")
            } else {
                url.clone()
            };
            let content = super::autolink::detect(&u).unwrap_or_else(|| link_domain(&u));
            links.push(TableLink {
                href: u,
                content,
                kind: LinkKind::Auto,
            });
        }
        Node::Image { text, destination } => {
            let content = if text.is_empty() {
                link_domain(destination)
            } else {
                text.clone()
            };
            images.push(TableLink {
                href: destination.clone(),
                content,
                kind: LinkKind::Image,
            });
        }
        Node::Link {
            destination,
            children,
        } => {
            links.push(TableLink {
                href: destination.clone(),
                content: children.iter().map(|c| c.text_content()).collect(),
                kind: LinkKind::Regular,
            });
            for c in children {
                collect_links(c, links, images);
            }
        }
        Node::Strikethrough { children, .. } => {
            for c in children {
                collect_links(c, links, images);
            }
        }
        other => {
            for c in other.children() {
                collect_links(c, links, images);
            }
        }
    }
}

/// The host of a URL, or `"link"` when it cannot be parsed.
pub fn link_domain(href: &str) -> String {
    match crate::deps::url::Url::parse(href) {
        Ok(u) => u.hostname(),
        Err(_) => "link".into(),
    }
}

/// Builds the OSC 8 open/close pair for a URL.
///
/// The id is the FNV-1a 32-bit hash of the link, exactly as Glamour computes it.
/// A link that is only a fragment is not turned into a hyperlink.
pub fn make_hyperlink(link: &str) -> (String, String, bool) {
    let parsed = crate::deps::url::Url::parse(link);
    let fragment_only = match &parsed {
        Ok(u) => format!("#{}", u.fragment()) == link,
        Err(_) => false,
    };
    let valid = parsed.is_ok() && !fragment_only;
    if !valid {
        return (String::new(), String::new(), false);
    }
    let id = fnv1a32(link);
    (
        format!("\u{1b}]8;id={id};{link}\u{7}"),
        "\u{1b}]8;;\u{7}".to_string(),
        true,
    )
}

fn fnv1a32(s: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for b in s.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

/// Resolves `rel` against `base`, leaving absolute URLs alone.
pub fn resolve_relative_url(base: &str, rel: &str) -> String {
    crate::deps::url::resolve_reference(base, rel)
}

/// Undoes markdown backslash escapes, as Glamour's `escapeReplacer` does.
///
/// goldmark hands the renderer the raw source segment, so `\.` still carries
/// its backslash; the replacement also reaches code blocks, which is where it
/// is observable.
fn unescape_markdown(s: &str) -> String {
    const ESCAPABLE: &str = "\\`*_{}[]<>()#+-.!|";
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if ESCAPABLE.contains(next) {
                    out.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

/// Expands a Glamour `format` template. Only `{{.text}}` is substituted.
fn format_token(format: &str, token: &str) -> String {
    format.replace("{{.text}}", token)
}

/// Strips tags from HTML and unescapes entities, as Glamour's sanitizer does.
fn sanitize_html(s: &str, trim_spaces: bool) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let out = unescape_entities(&out);
    if trim_spaces {
        out.trim().to_string()
    } else {
        out
    }
}

/// Unescapes the HTML entities `html.UnescapeString` resolves in practice.
fn unescape_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(end) = s[i..].find(';').map(|e| i + e + 1) {
                let entity = &s[i..end];
                if let Some(rep) = entity_value(entity) {
                    out.push_str(&rep);
                    i = end;
                    continue;
                }
            }
        }
        let c = s[i..].chars().next().expect("valid char boundary");
        out.push(c);
        i += c.len_utf8();
    }
    out
}

fn entity_value(entity: &str) -> Option<String> {
    let inner = entity.strip_prefix('&')?.strip_suffix(';')?;
    if let Some(num) = inner.strip_prefix('#') {
        let code = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            num.parse::<u32>().ok()?
        };
        return char::from_u32(code).map(|c| c.to_string());
    }
    let c = match inner {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{00A0}',
        "hellip" => '…',
        "mdash" => '—',
        "ndash" => '–',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        _ => return None,
    };
    Some(c.to_string())
}

/// Title-cases each word, as `golang.org/x/text/cases.Title` does for English.
fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start_of_word = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if start_of_word {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start_of_word = false;
        } else {
            out.push(c);
            start_of_word = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deps::glamour::styles;

    fn notty(md: &str, width: i64) -> String {
        let opts = Options {
            word_wrap: width,
            styles: styles::default_style(styles::NOTTY_STYLE).expect("notty style"),
            ..Default::default()
        };
        Renderer::new(opts).render(md)
    }

    #[test]
    fn renders_a_heading_padded_to_the_block_width() {
        let out = notty("# Heading One\n", 80);
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines[0], "");
        assert_eq!(lines[1], format!("  # Heading One{}", " ".repeat(63)));
    }

    #[test]
    fn word_wrap_zero_disables_padding() {
        let out = notty("# Heading One\n", 0);
        assert_eq!(out, "\n  # Heading One\n\n");
    }

    #[test]
    fn renders_unordered_and_ordered_items() {
        let out = notty("- one\n- two\n", 0);
        assert!(out.contains("• one"), "{out:?}");
        let out = notty("1. first\n2. second\n", 0);
        assert!(out.contains("1. first"), "{out:?}");
        assert!(out.contains("2. second"), "{out:?}");
    }

    #[test]
    fn renders_task_markers() {
        let out = notty("- [x] done\n- [ ] todo\n", 0);
        assert!(out.contains("[x] done"), "{out:?}");
        assert!(out.contains("[ ] todo"), "{out:?}");
    }

    #[test]
    fn renders_horizontal_rule() {
        let out = notty("a\n\n---\n\nb\n", 0);
        assert!(out.contains("--------"), "{out:?}");
    }

    #[test]
    fn renders_block_quote_with_indent_token() {
        let out = notty("> quoted\n", 0);
        assert!(out.contains("| quoted"), "{out:?}");
    }

    #[test]
    fn emphasis_uses_style_affixes() {
        let out = notty("*em* and **strong** and ~~gone~~\n", 0);
        assert!(out.contains("*em*"), "{out:?}");
        assert!(out.contains("**strong**"), "{out:?}");
        assert!(out.contains("~~gone~~"), "{out:?}");
    }

    #[test]
    fn code_span_drops_backticks_in_notty() {
        let out = notty("a `code` b\n", 0);
        assert!(out.contains("a code b"), "{out:?}");
    }

    #[test]
    fn paragraph_joins_soft_breaks_unless_preserved() {
        assert!(notty("one\ntwo\n", 0).contains("one two"));
        let opts = Options {
            word_wrap: 0,
            preserve_new_lines: true,
            styles: styles::default_style(styles::NOTTY_STYLE).expect("notty style"),
            ..Default::default()
        };
        let out = Renderer::new(opts).render("one\ntwo\n");
        assert!(out.contains("one\n  two"), "{out:?}");
    }

    #[test]
    fn hyperlink_id_is_the_fnv1a_hash() {
        let (open, close, valid) = make_hyperlink("https://example.com");
        assert!(valid);
        assert_eq!(open, "\u{1b}]8;id=1874592979;https://example.com\u{7}");
        assert_eq!(close, "\u{1b}]8;;\u{7}");
    }

    #[test]
    fn fragment_only_links_are_not_hyperlinked() {
        let (_, _, valid) = make_hyperlink("#section");
        assert!(!valid);
    }

    #[test]
    fn image_uses_the_format_template() {
        let out = notty("![alt](https://example.com/a.png)\n", 0);
        assert!(out.contains("Image: "), "{out:?}");
        assert!(out.contains("alt"), "{out:?}");
        assert!(out.contains(" → "), "{out:?}");
    }

    #[test]
    fn sanitizes_html_blocks() {
        assert_eq!(sanitize_html("<p>hi &amp; bye</p>", true), "hi & bye");
    }

    #[test]
    fn title_case_capitalises_words() {
        assert_eq!(title_case("hello wide world"), "Hello Wide World");
    }
}
