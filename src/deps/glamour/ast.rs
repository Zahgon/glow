//! The markdown document model the renderer walks.
//!
//! `pulldown-cmark` emits a flat event stream; Glamour's renderer needs a tree
//! with sibling awareness (a heading knows whether it is first, a list item
//! knows its ordinal, a link knows whether it sits inside a table). This module
//! rebuilds that tree, with the same node set `goldmark` handed to Glamour.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};

/// Column alignment of a table column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    /// No explicit alignment.
    None,
    /// Left-aligned.
    Left,
    /// Centre-aligned.
    Center,
    /// Right-aligned.
    Right,
}

/// A node in the markdown document.
#[derive(Debug, Clone)]
pub enum Node {
    /// The document root.
    Document(Vec<Node>),
    /// An ATX or setext heading.
    Heading { level: usize, children: Vec<Node> },
    /// A paragraph.
    Paragraph(Vec<Node>),
    /// A block quote.
    BlockQuote(Vec<Node>),
    /// An ordered or bulleted list.
    List {
        ordered: bool,
        start: u64,
        items: Vec<Node>,
    },
    /// One item of a list.
    ListItem {
        children: Vec<Node>,
        task: Option<bool>,
    },
    /// A fenced or indented code block.
    CodeBlock { code: String, language: String },
    /// A block of raw HTML.
    HtmlBlock(String),
    /// A thematic break.
    ThematicBreak,
    /// A GFM table.
    Table {
        alignments: Vec<Alignment>,
        head: Vec<Vec<Node>>,
        rows: Vec<Vec<Vec<Node>>>,
    },
    /// A definition list.
    DefinitionList(Vec<Node>),
    /// The term of a definition list entry.
    DefinitionTerm(Vec<Node>),
    /// The description of a definition list entry.
    DefinitionDescription(Vec<Node>),
    /// Literal text. A trailing `\n` marks a soft or hard line break.
    Text(String),
    /// An inline code span.
    CodeSpan(String),
    /// Emphasis; level 1 is `em`, level 2 is `strong`.
    Emphasis { level: usize, children: Vec<Node> },
    /// Strikethrough text. Glamour renders only the flattened text, but the
    /// children are kept so a link inside one still reaches a table's footnote
    /// list.
    Strikethrough { text: String, children: Vec<Node> },
    /// An inline link.
    Link {
        destination: String,
        children: Vec<Node>,
    },
    /// An autolink; `is_email` distinguishes `<a@b.c>` from `<https://…>`.
    AutoLink { url: String, is_email: bool },
    /// An image.
    Image { text: String, destination: String },
    /// Inline raw HTML.
    RawHtml(String),
}

impl Node {
    /// Children of a container node, or an empty slice.
    pub fn children(&self) -> &[Node] {
        match self {
            Node::Document(c)
            | Node::Paragraph(c)
            | Node::BlockQuote(c)
            | Node::DefinitionList(c)
            | Node::DefinitionTerm(c)
            | Node::DefinitionDescription(c) => c,
            Node::Heading { children, .. }
            | Node::ListItem { children, .. }
            | Node::Emphasis { children, .. }
            | Node::Link { children, .. } => children,
            Node::List { items, .. } => items,
            _ => &[],
        }
    }

    /// Concatenated literal text of this node's subtree.
    ///
    /// Mirrors Glamour's `nodeContent`, which walks to the `Text` leaves.
    pub fn text_content(&self) -> String {
        match self {
            Node::Text(t) => t.clone(),
            Node::CodeSpan(t) | Node::RawHtml(t) | Node::HtmlBlock(t) => t.clone(),
            Node::Strikethrough { text, .. } => text.clone(),
            _ => self.children().iter().map(|c| c.text_content()).collect(),
        }
    }
}

/// Parses `source` into a document tree.
pub fn parse(source: &str) -> Node {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_DEFINITION_LIST);
    opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);

    let parser = Parser::new_ext(source, opts).into_offset_iter();
    let mut builder = Builder::new();
    for (event, range) in parser {
        // `pulldown_cmark` reports a backslash-escaped character with a range
        // that starts after the backslash, so the escape has to be looked for
        // in what comes before it.
        let escaped_start = trailing_backslashes(&source[..range.start]) % 2 == 1;
        let raw = source.get(range).unwrap_or_default();
        builder.handle(event, raw, escaped_start);
    }
    let tree = split_line_tails(linkify_nodes(builder.finish()));
    Node::Document(tree.into_iter().map(unshield_node).collect())
}

/// Splits the text run that ends each source line at its last space.
///
/// goldmark closes a line's final text segment at the last inline-parser
/// trigger before the line break, so `"aaa bbb"` reaches the renderer as the
/// two nodes `"aaa"` and `" bbb"`. Each becomes its own styled run in the
/// output, which is why the split has to be reproduced rather than normalised
/// away.
fn split_line_tails(nodes: Vec<Node>) -> Vec<Node> {
    nodes.into_iter().map(split_in_node).collect()
}

fn split_in_node(node: Node) -> Node {
    match node {
        Node::Paragraph(c) => Node::Paragraph(split_inlines(c)),
        Node::Heading { level, children } => Node::Heading {
            level,
            children: split_inlines(children),
        },
        Node::ListItem { children, task } => Node::ListItem {
            children: split_inlines(children),
            task,
        },
        Node::DefinitionTerm(c) => Node::DefinitionTerm(split_inlines(c)),
        Node::DefinitionDescription(c) => Node::DefinitionDescription(split_inlines(c)),
        Node::BlockQuote(c) => Node::BlockQuote(split_line_tails(c)),
        Node::List {
            ordered,
            start,
            items,
        } => Node::List {
            ordered,
            start,
            items: split_line_tails(items),
        },
        Node::DefinitionList(c) => Node::DefinitionList(split_line_tails(c)),
        Node::Document(c) => Node::Document(split_line_tails(c)),
        Node::Table {
            alignments,
            head,
            rows,
        } => Node::Table {
            alignments,
            head: head.into_iter().map(split_inlines).collect(),
            rows: rows
                .into_iter()
                .map(|r| r.into_iter().map(split_inlines).collect())
                .collect(),
        },
        other => other,
    }
}

/// Splits text runs at the delimiter characters that end a goldmark text
/// segment: a run of `*`, `_` or `~`, or a `[`.
///
/// Each resulting node is styled separately, so an emphasis containing an
/// underscore renders its affixes more than once — `**XDG_CACHE**` becomes
/// `**XDG_****CACHE**`. Reproducing the segmentation is what makes that match.
fn split_delimiters(children: Vec<Node>) -> Vec<Node> {
    let mut out = Vec::with_capacity(children.len());
    for child in children {
        match child {
            Node::Text(t) => out.extend(split_text_delimiters(&t).into_iter().map(Node::Text)),
            Node::Emphasis { level, children } => out.push(Node::Emphasis {
                level,
                children: split_delimiters(children),
            }),
            other => out.push(other),
        }
    }
    out
}

fn is_delimiter(c: char) -> bool {
    c == '*' || c == '_' || c == '~'
}

fn split_text_delimiters(t: &str) -> Vec<String> {
    let chars: Vec<char> = t.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if is_delimiter(c) {
            // Consume the whole delimiter run, then break after it.
            while i < chars.len() && is_delimiter(chars[i]) {
                cur.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                parts.push(std::mem::take(&mut cur));
            }
            continue;
        }
        cur.push(c);
        i += 1;
    }
    if !cur.is_empty() || parts.is_empty() {
        parts.push(cur);
    }
    parts
}

/// Whether a node starts a new block, ending the line before it.
fn is_block(node: &Node) -> bool {
    matches!(
        node,
        Node::List { .. }
            | Node::Paragraph(_)
            | Node::BlockQuote(_)
            | Node::CodeBlock { .. }
            | Node::Table { .. }
            | Node::Heading { .. }
            | Node::DefinitionList(_)
            | Node::ThematicBreak
    )
}

/// Applies the split to the inline children of one block.
fn split_inlines(children: Vec<Node>) -> Vec<Node> {
    split_inlines_at(children, true)
}

/// `top` marks the direct inline children of a block. Only there does the
/// final text run get split; a run that ends a line is split at any depth.
fn split_inlines_at(children: Vec<Node>, top: bool) -> Vec<Node> {
    let last = children.len().saturating_sub(1);
    let block_after: Vec<bool> = (0..children.len())
        .map(|i| children.get(i + 1).map(is_block).unwrap_or(false))
        .collect();
    let mut out = Vec::with_capacity(children.len() + 2);
    for (i, child) in children.into_iter().enumerate() {
        let next_is_block = block_after[i];
        match child {
            Node::Emphasis { level, children } => out.push(Node::Emphasis {
                level,
                children: split_inlines_at(split_delimiters(children), false),
            }),
            Node::Link {
                destination,
                children,
            } => out.push(Node::Link {
                destination,
                children: split_inlines_at(children, false),
            }),
            Node::Text(t) => {
                let (body, newline) = match t.strip_suffix('\n') {
                    Some(b) => (b.to_string(), "\n"),
                    None => (t.clone(), ""),
                };
                // Only a run that ends its line is split at the tail: one
                // carrying a line break, the last inline of a block, or one
                // followed by a nested block. Link labels split every run.
                let ends_line = !newline.is_empty() || (top && (i == last || next_is_block));

                let mut pieces = split_text_brackets(&body);
                let tail = pieces.pop().expect("at least one piece");
                for p in pieces {
                    out.push(Node::Text(p));
                }
                if !ends_line {
                    out.push(Node::Text(format!("{tail}{newline}")));
                    continue;
                }
                match last_trigger(&tail) {
                    Some(idx) if idx > 0 => {
                        out.push(Node::Text(tail[..idx].to_string()));
                        out.push(Node::Text(format!("{}{}", &tail[idx..], newline)));
                    }
                    _ => out.push(Node::Text(format!("{tail}{newline}"))),
                }
            }
            // Text inside a link or emphasis is one segment in goldmark, so
            // the split never reaches inside them.
            other => out.push(split_in_node(other)),
        }
    }
    out
}

/// How many backslashes `s` ends with.
fn trailing_backslashes(s: &str) -> usize {
    s.chars().rev().take_while(|c| *c == '\\').count()
}

/// The markup characters the segmenting passes look for, and the private-use
/// code points that stand in for them.
///
/// A character that reached the tree through an escape or an entity is not
/// markup, but by the time `pulldown_cmark` reports it the spelling is gone.
/// Standing it in for the duration of the segmenting passes preserves the
/// distinction; [`unshield`] puts the real character back before rendering.
const SHIELDED: [(char, char); 5] = [
    ('[', '\u{e000}'),
    (']', '\u{e001}'),
    ('*', '\u{e002}'),
    ('_', '\u{e003}'),
    ('~', '\u{e004}'),
];

/// Replaces markup characters with their stand-ins.
fn shield(s: &str) -> String {
    s.chars()
        .map(|c| {
            SHIELDED
                .iter()
                .find(|(from, _)| *from == c)
                .map(|(_, to)| *to)
                .unwrap_or(c)
        })
        .collect()
}

/// Puts the real characters back.
fn unshield(s: &str) -> String {
    if !s.chars().any(|c| ('\u{e000}'..='\u{e004}').contains(&c)) {
        return s.to_string();
    }
    s.chars()
        .map(|c| {
            SHIELDED
                .iter()
                .find(|(_, to)| *to == c)
                .map(|(from, _)| *from)
                .unwrap_or(c)
        })
        .collect()
}

/// Restores shielded characters throughout a subtree.
fn unshield_node(node: Node) -> Node {
    fn all(nodes: Vec<Node>) -> Vec<Node> {
        nodes.into_iter().map(unshield_node).collect()
    }
    match node {
        Node::Text(t) => Node::Text(unshield(&t)),
        Node::CodeSpan(t) => Node::CodeSpan(unshield(&t)),
        Node::RawHtml(t) => Node::RawHtml(unshield(&t)),
        Node::Strikethrough { text, children } => Node::Strikethrough {
            text: unshield(&text),
            children: all(children),
        },
        Node::Image { text, destination } => Node::Image {
            text: unshield(&text),
            destination,
        },
        Node::Document(c) => Node::Document(all(c)),
        Node::Paragraph(c) => Node::Paragraph(all(c)),
        Node::BlockQuote(c) => Node::BlockQuote(all(c)),
        Node::DefinitionList(c) => Node::DefinitionList(all(c)),
        Node::DefinitionTerm(c) => Node::DefinitionTerm(all(c)),
        Node::DefinitionDescription(c) => Node::DefinitionDescription(all(c)),
        Node::Heading { level, children } => Node::Heading {
            level,
            children: all(children),
        },
        Node::ListItem { children, task } => Node::ListItem {
            children: all(children),
            task,
        },
        Node::List {
            ordered,
            start,
            items,
        } => Node::List {
            ordered,
            start,
            items: all(items),
        },
        Node::Emphasis { level, children } => Node::Emphasis {
            level,
            children: all(children),
        },
        Node::Link {
            destination,
            children,
        } => Node::Link {
            destination,
            children: all(children),
        },
        Node::Table {
            alignments,
            head,
            rows,
        } => Node::Table {
            alignments,
            head: head.into_iter().map(all).collect(),
            rows: rows
                .into_iter()
                .map(|r| r.into_iter().map(all).collect())
                .collect(),
        },
        other => other,
    }
}

/// Splits a text run around the link-label nodes goldmark leaves behind.
///
/// `[` (and `![`) start a link label, which the parser inserts as a node of
/// its own; the text before it is flushed. When the label finds no closing
/// `]`, that node survives and is styled separately. When a `]` arrives and no
/// link forms, the opener is merged back into the text before it, and the `]`
/// itself joins the text that follows — which is why `a [b] c` reaches the
/// renderer as `a [`, `b]` and ` c`.
fn split_text_brackets(run: &str) -> Vec<String> {
    /// One node under construction: a piece of text, or a link-label opener.
    struct Piece {
        text: String,
        opener: bool,
    }

    let mut nodes: Vec<Piece> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut acc = String::new();

    // Merges into the last node when it can, which is what makes every flush
    // but the last one invisible.
    fn flush(nodes: &mut Vec<Piece>, acc: &mut String) {
        if acc.is_empty() {
            return;
        }
        match nodes.last_mut() {
            Some(last) if !last.opener => last.text.push_str(acc),
            _ => nodes.push(Piece {
                text: acc.clone(),
                opener: false,
            }),
        }
        acc.clear();
    }

    let chars: Vec<char> = run.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '[' || (c == '!' && chars.get(i + 1) == Some(&'[')) {
            let label = if c == '[' { "[" } else { "![" };
            i += label.chars().count();
            flush(&mut nodes, &mut acc);
            nodes.push(Piece {
                text: label.to_string(),
                opener: true,
            });
            open.push(nodes.len() - 1);
            continue;
        }
        if c == ']' {
            flush(&mut nodes, &mut acc);
            if let Some(bi) = open.pop() {
                if bi > 0 && !nodes[bi - 1].opener {
                    let label = nodes[bi].text.clone();
                    nodes[bi - 1].text.push_str(&label);
                    nodes.remove(bi);
                    for o in &mut open {
                        if *o > bi {
                            *o -= 1;
                        }
                    }
                } else {
                    nodes[bi].opener = false;
                }
            }
            acc.push(']');
            i += 1;
            continue;
        }
        acc.push(c);
        i += 1;
    }
    flush(&mut nodes, &mut acc);

    if nodes.is_empty() {
        return vec![String::new()];
    }
    nodes.into_iter().map(|p| p.text).collect()
}

/// Characters that end a line's final text segment.
///
/// goldmark's inline loop flushes the text it has accumulated before it tries
/// an inline parser, and it consults one for every space and for the
/// punctuation those parsers are registered under. The flush merges into the
/// previous node, so the only boundary that survives is the last one on the
/// line — the end-of-line flush always appends a segment of its own.
///
/// Only whitespace and `]` are treated as triggers here. `[` is handled by
/// [`split_text_brackets`] and `*`, `_` and `~` by [`split_text_delimiters`],
/// both of which see the whole run. The remaining trigger characters —
/// `` ` ``, `!`, `<` and `(` — are left out: they end a segment far less often
/// than they appear, and every one of them can also arrive through an escape
/// or an entity, where goldmark would not have flushed at all.
fn is_trigger(c: char) -> bool {
    c.is_whitespace() || c == ']'
}

/// The byte offset of the last trigger character in `body`.
///
/// The text from there to the end of the line is what the end-of-line flush
/// appends as a segment of its own.
fn last_trigger(body: &str) -> Option<usize> {
    body.char_indices()
        .rfind(|(_, c)| is_trigger(*c))
        .map(|(i, _)| i)
}

/// A stack frame while rebuilding the tree.
struct Frame {
    tag: Option<Tag<'static>>,
    children: Vec<Node>,
    /// Cells of the row currently being built, for tables.
    row: Vec<Vec<Node>>,
    /// Completed header cells, for tables.
    head: Vec<Vec<Node>>,
    /// Completed body rows, for tables.
    rows: Vec<Vec<Vec<Node>>>,
    in_head: bool,
}

impl Frame {
    fn new(tag: Option<Tag<'static>>) -> Self {
        Frame {
            tag,
            children: Vec::new(),
            row: Vec::new(),
            head: Vec::new(),
            rows: Vec::new(),
            in_head: false,
        }
    }
}

struct Builder {
    stack: Vec<Frame>,
    /// Pending task-list marker for the list item being built.
    task: Option<bool>,
}

impl Builder {
    fn new() -> Self {
        Builder {
            stack: vec![Frame::new(None)],
            task: None,
        }
    }

    fn push_node(&mut self, n: Node) {
        self.stack
            .last_mut()
            .expect("non-empty stack")
            .children
            .push(n);
    }

    /// Appends a line break to the previous text node, the way goldmark folds
    /// soft and hard breaks into the text segment that precedes them.
    fn push_break(&mut self) {
        let frame = self.stack.last_mut().expect("non-empty stack");
        match frame.children.last_mut() {
            Some(Node::Text(t)) => t.push('\n'),
            _ => frame.children.push(Node::Text("\n".into())),
        }
    }

    fn handle(&mut self, event: Event<'_>, raw: &str, escaped_start: bool) {
        match event {
            Event::Start(tag) => {
                let owned: Tag<'static> = own_tag(&tag);
                let mut frame = Frame::new(Some(owned));
                if matches!(tag, Tag::TableHead) {
                    frame.in_head = true;
                }
                self.stack.push(frame);
            }
            Event::End(end) => self.close(end),
            Event::Text(t) => {
                // A run that does not read back as its own source came from a
                // backslash escape or an HTML entity, so goldmark never saw the
                // character it decodes to. Shielding it keeps the segmenting
                // passes from treating it as markup.
                let text = if raw != t.as_ref() {
                    // An entity: the whole run stands for other spelling.
                    shield(&t)
                } else if escaped_start {
                    // A backslash escape shields only the character it covers.
                    let mut chars = t.chars();
                    match chars.next() {
                        Some(first) => format!("{}{}", shield(&first.to_string()), chars.as_str()),
                        None => t.to_string(),
                    }
                } else {
                    t.to_string()
                };
                let frame = self.stack.last_mut().expect("non-empty stack");
                match frame.children.last_mut() {
                    Some(Node::Text(prev)) if !prev.ends_with('\n') => prev.push_str(&text),
                    _ => frame.children.push(Node::Text(text)),
                }
            }
            Event::Code(t) => self.push_node(Node::CodeSpan(t.to_string())),
            Event::Html(h) => self.push_node(Node::HtmlBlock(h.to_string())),
            Event::InlineHtml(h) => self.push_node(Node::RawHtml(h.to_string())),
            Event::SoftBreak | Event::HardBreak => self.push_break(),
            Event::Rule => self.push_node(Node::ThematicBreak),
            Event::TaskListMarker(checked) => self.task = Some(checked),
            Event::FootnoteReference(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {}
        }
    }

    fn close(&mut self, end: TagEnd) {
        let frame = self.stack.pop().expect("unbalanced tags");
        let tag = frame
            .tag
            .clone()
            .expect("closing tag without an opening one");
        let children = frame.children;

        match tag {
            Tag::Paragraph => self.push_node(Node::Paragraph(children)),
            Tag::Heading { level, .. } => self.push_node(Node::Heading {
                level: heading_level(level),
                children,
            }),
            Tag::BlockQuote(_) => self.push_node(Node::BlockQuote(children)),
            Tag::CodeBlock(kind) => {
                let language = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().unwrap_or("").to_string()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                let code = children
                    .iter()
                    .map(|c| match c {
                        Node::Text(t) => t.clone(),
                        other => other.text_content(),
                    })
                    .collect();
                self.push_node(Node::CodeBlock { code, language });
            }
            Tag::List(start) => self.push_node(Node::List {
                ordered: start.is_some(),
                start: start.unwrap_or(1),
                items: children,
            }),
            Tag::Item => {
                let task = self.task.take();
                // A loose list item wraps its content in paragraphs. Glamour
                // renders a paragraph inside a list item as nothing, so the
                // wrapper is spliced away here instead.
                let mut flat = Vec::with_capacity(children.len());
                for child in children {
                    match child {
                        Node::Paragraph(inner) => flat.extend(inner),
                        other => flat.push(other),
                    }
                }
                self.push_node(Node::ListItem {
                    children: flat,
                    task,
                });
            }
            Tag::Emphasis => self.push_node(Node::Emphasis { level: 1, children }),
            Tag::Strong => self.push_node(Node::Emphasis { level: 2, children }),
            Tag::Strikethrough => {
                let text = children.iter().map(|c| c.text_content()).collect();
                self.push_node(Node::Strikethrough { text, children });
            }
            Tag::Link {
                link_type,
                dest_url,
                ..
            } => match link_type {
                LinkType::Autolink => self.push_node(Node::AutoLink {
                    url: dest_url.to_string(),
                    is_email: false,
                }),
                LinkType::Email => self.push_node(Node::AutoLink {
                    url: dest_url.to_string(),
                    is_email: true,
                }),
                _ => self.push_node(Node::Link {
                    destination: dest_url.to_string(),
                    children,
                }),
            },
            Tag::Image { dest_url, .. } => {
                let text = children.iter().map(|c| c.text_content()).collect();
                self.push_node(Node::Image {
                    text,
                    destination: dest_url.to_string(),
                });
            }
            Tag::Table(aligns) => {
                let alignments = aligns.iter().map(|a| alignment(*a)).collect();
                self.push_node(Node::Table {
                    alignments,
                    head: frame.head,
                    rows: frame.rows,
                });
            }
            Tag::TableHead | Tag::TableRow => {
                let row = frame.row;
                let parent = self.stack.last_mut().expect("table row outside a table");
                if frame.in_head {
                    parent.head = row;
                } else {
                    parent.rows.push(row);
                }
            }
            Tag::TableCell => {
                let parent = self.stack.last_mut().expect("cell outside a row");
                parent.row.push(children);
            }
            Tag::DefinitionList => self.push_node(Node::DefinitionList(children)),
            Tag::DefinitionListTitle => self.push_node(Node::DefinitionTerm(children)),
            Tag::DefinitionListDefinition => self.push_node(Node::DefinitionDescription(children)),
            Tag::HtmlBlock => {
                let text = children.iter().map(|c| c.text_content()).collect();
                self.push_node(Node::HtmlBlock(text));
            }
            _ => {
                for c in children {
                    self.push_node(c);
                }
            }
        }
        let _ = end;
    }

    fn finish(mut self) -> Vec<Node> {
        let root = self.stack.pop().expect("unbalanced document");
        root.children
    }
}

/// The head frame of a table needs its `in_head` flag before its rows close.
fn own_tag(tag: &Tag<'_>) -> Tag<'static> {
    // `Tag` borrows from the source; cloning into owned `CowStr`s keeps the
    // frame independent of the parser's lifetime.
    match tag {
        Tag::Paragraph => Tag::Paragraph,
        Tag::Heading { level, .. } => Tag::Heading {
            level: *level,
            id: None,
            classes: vec![],
            attrs: vec![],
        },
        Tag::BlockQuote(k) => Tag::BlockQuote(*k),
        Tag::CodeBlock(k) => Tag::CodeBlock(match k {
            CodeBlockKind::Indented => CodeBlockKind::Indented,
            CodeBlockKind::Fenced(s) => CodeBlockKind::Fenced(s.to_string().into()),
        }),
        Tag::HtmlBlock => Tag::HtmlBlock,
        Tag::List(s) => Tag::List(*s),
        Tag::Item => Tag::Item,
        Tag::FootnoteDefinition(s) => Tag::FootnoteDefinition(s.to_string().into()),
        Tag::DefinitionList => Tag::DefinitionList,
        Tag::DefinitionListTitle => Tag::DefinitionListTitle,
        Tag::DefinitionListDefinition => Tag::DefinitionListDefinition,
        Tag::Table(a) => Tag::Table(a.clone()),
        Tag::TableHead => Tag::TableHead,
        Tag::TableRow => Tag::TableRow,
        Tag::TableCell => Tag::TableCell,
        Tag::Emphasis => Tag::Emphasis,
        Tag::Strong => Tag::Strong,
        Tag::Strikethrough => Tag::Strikethrough,
        Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        } => Tag::Link {
            link_type: *link_type,
            dest_url: dest_url.to_string().into(),
            title: title.to_string().into(),
            id: id.to_string().into(),
        },
        Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        } => Tag::Image {
            link_type: *link_type,
            dest_url: dest_url.to_string().into(),
            title: title.to_string().into(),
            id: id.to_string().into(),
        },
        Tag::MetadataBlock(k) => Tag::MetadataBlock(*k),
    }
}

fn heading_level(l: HeadingLevel) -> usize {
    match l {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn alignment(a: pulldown_cmark::Alignment) -> Alignment {
    match a {
        pulldown_cmark::Alignment::None => Alignment::None,
        pulldown_cmark::Alignment::Left => Alignment::Left,
        pulldown_cmark::Alignment::Center => Alignment::Center,
        pulldown_cmark::Alignment::Right => Alignment::Right,
    }
}

/// Turns bare URLs and email addresses in text into autolinks.
///
/// `goldmark`'s GFM bundle enables Linkify, which `pulldown-cmark` does not
/// implement, so the pass is done here on the assembled tree instead.
fn linkify_nodes(nodes: Vec<Node>) -> Vec<Node> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            Node::Text(t) => out.extend(linkify_text(&t)),
            Node::Document(c) => out.push(Node::Document(linkify_nodes(c))),
            Node::Paragraph(c) => out.push(Node::Paragraph(linkify_nodes(c))),
            Node::BlockQuote(c) => out.push(Node::BlockQuote(linkify_nodes(c))),
            Node::DefinitionList(c) => out.push(Node::DefinitionList(linkify_nodes(c))),
            Node::DefinitionTerm(c) => out.push(Node::DefinitionTerm(linkify_nodes(c))),
            Node::DefinitionDescription(c) => {
                out.push(Node::DefinitionDescription(linkify_nodes(c)))
            }
            Node::Heading { level, children } => out.push(Node::Heading {
                level,
                children: linkify_nodes(children),
            }),
            Node::ListItem { children, task } => out.push(Node::ListItem {
                children: linkify_nodes(children),
                task,
            }),
            Node::List {
                ordered,
                start,
                items,
            } => out.push(Node::List {
                ordered,
                start,
                items: linkify_nodes(items),
            }),
            Node::Emphasis { level, children } => out.push(Node::Emphasis {
                level,
                children: linkify_nodes(children),
            }),
            Node::Table {
                alignments,
                head,
                rows,
            } => out.push(Node::Table {
                alignments,
                head: head.into_iter().map(linkify_nodes).collect(),
                rows: rows
                    .into_iter()
                    .map(|r| r.into_iter().map(linkify_nodes).collect())
                    .collect(),
            }),
            // Link children are never linkified again.
            other => out.push(other),
        }
    }
    out
}

/// Splits one text run into text and autolink nodes.
fn linkify_text(text: &str) -> Vec<Node> {
    let bytes = text.as_bytes();
    let mut out: Vec<Node> = Vec::new();
    let mut plain = String::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let at_boundary = i == 0 || !is_url_body(text[..i].chars().next_back().unwrap_or(' '));
        if at_boundary {
            if let Some(end) = scheme_end(text, i) {
                if !plain.is_empty() {
                    out.push(Node::Text(std::mem::take(&mut plain)));
                }
                // A `www.` match carries an implicit protocol, the way
                // goldmark's `AutoLink.URL` prepends one when it recorded a
                // protocol for the match.
                let matched = &text[i..end];
                let url = if matched.starts_with("www.") {
                    format!("http://{matched}")
                } else {
                    matched.to_string()
                };
                out.push(Node::AutoLink {
                    url,
                    is_email: false,
                });
                i = end;
                continue;
            }
        }
        if bytes[i] == b'@' {
            if let Some((start, end)) = email_span(text, i) {
                let consumed = i - start;
                plain.truncate(plain.len() - consumed);
                if !plain.is_empty() {
                    out.push(Node::Text(std::mem::take(&mut plain)));
                }
                out.push(Node::AutoLink {
                    url: text[start..end].to_string(),
                    is_email: true,
                });
                i = end;
                continue;
            }
        }
        let ch = text[i..].chars().next().expect("valid char boundary");
        plain.push(ch);
        i += ch.len_utf8();
    }

    if !plain.is_empty() {
        out.push(Node::Text(plain));
    }
    out
}

fn is_url_body(c: char) -> bool {
    !c.is_whitespace() && c != '<' && c != '(' && c != '[' && c != '"' && c != '\''
}

/// Returns the end offset of a bare `http(s)://` or `www.` URL starting at `i`.
fn scheme_end(text: &str, i: usize) -> Option<usize> {
    let rest = &text[i..];
    let prefix_len = if rest.starts_with("https://") {
        8
    } else if rest.starts_with("http://") {
        7
    } else if rest.starts_with("www.") {
        4
    } else {
        return None;
    };

    let body_start = i + prefix_len;
    let mut end = text.len();
    for (off, c) in text[body_start..].char_indices() {
        if c.is_whitespace() || c == '<' || c == '>' || c == '"' || c == '\'' {
            end = body_start + off;
            break;
        }
    }
    // GFM requires the domain to carry at least one period, which is why
    // `http://localhost:80` is left as plain text.
    let host_end = text[body_start..end]
        .find(['/', '?', '#'])
        .map(|o| body_start + o)
        .unwrap_or(end);
    let host = &text[body_start..host_end];
    let host = host.split(':').next().unwrap_or(host);
    if !host.contains('.') {
        return None;
    }
    finish_url(text, i, end)
}

/// Trims the trailing punctuation goldmark's linkifier excludes from a URL.
fn finish_url(text: &str, start: usize, mut end: usize) -> Option<usize> {
    if end.saturating_sub(start) <= 8 {
        return None;
    }
    while end > start {
        let c = text[..end].chars().next_back()?;
        match c {
            '.' | ',' | ':' | ';' | '!' | '?' | '*' | '_' | '~' => end -= c.len_utf8(),
            ')' => {
                let opens = text[start..end].matches('(').count();
                let closes = text[start..end].matches(')').count();
                if closes > opens {
                    end -= 1;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    if end > start {
        Some(end)
    } else {
        None
    }
}

fn is_email_local(c: char) -> bool {
    c.is_ascii_alphanumeric() || ".-_+".contains(c)
}

fn is_email_domain(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// Returns the byte span of a bare email address whose `@` sits at `at`.
///
/// Follows the GFM autolink rules goldmark implements: the local part is
/// alphanumerics plus `.-_+`, the domain is dot-separated alphanumeric runs
/// with at least one dot, and the address may not end in `-` or `_`.
fn email_span(text: &str, at: usize) -> Option<(usize, usize)> {
    let mut start = at;
    for (off, c) in text[..at].char_indices().rev() {
        if is_email_local(c) {
            start = off;
        } else {
            break;
        }
    }
    if start == at {
        return None;
    }

    let mut end = at + 1;
    let mut has_dot = false;
    for (off, c) in text[at + 1..].char_indices() {
        if is_email_domain(c) {
            end = at + 1 + off + c.len_utf8();
        } else if c == '.' {
            has_dot = true;
            end = at + 1 + off + c.len_utf8();
        } else {
            break;
        }
    }
    while end > at + 1 {
        let last = text[..end].chars().next_back()?;
        if last == '.' || last == '-' || last == '_' {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    if !has_dot || end <= at + 1 {
        return None;
    }
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_labels_split_a_text_run() {
        assert_eq!(split_text_brackets("a [b] c"), vec!["a [", "b] c"]);
        assert_eq!(split_text_brackets("[a] b"), vec!["[", "a] b"]);
        assert_eq!(
            split_text_brackets("tags: [a, b]"),
            vec!["tags: [", "a, b]"]
        );
        assert_eq!(
            split_text_brackets("a [b [c] d] e"),
            vec!["a [", "b [", "c] d] e"]
        );
    }

    #[test]
    fn an_unclosed_label_is_a_run_of_its_own() {
        assert_eq!(split_text_brackets("a [ b"), vec!["a ", "[", " b"]);
        assert_eq!(split_text_brackets("a ["), vec!["a ", "["]);
        assert_eq!(split_text_brackets("["), vec!["["]);
        assert_eq!(split_text_brackets("a ![b"), vec!["a ", "![", "b"]);
    }

    #[test]
    fn a_closer_without_an_opener_stays_in_the_text() {
        assert_eq!(split_text_brackets("a ] b"), vec!["a ] b"]);
        assert_eq!(split_text_brackets("a ]] b"), vec!["a ]] b"]);
        assert_eq!(split_text_brackets("plain"), vec!["plain"]);
    }

    #[test]
    fn escapes_and_entities_are_shielded_from_the_segmenting_passes() {
        // The rendered text is the same either way; what changes is where the
        // renderer starts and stops a styled run.
        let escaped = parse("# a \\[b\\] c\n");
        let literal = parse("# a [b] c\n");
        assert_eq!(escaped.text_content(), literal.text_content());
        let count = |n: &Node| n.children()[0].children().len();
        assert!(
            count(&escaped) < count(&literal),
            "an escaped bracket does not open a link label"
        );
    }

    #[test]
    fn parses_headings_and_paragraphs() {
        let doc = parse("# Title\n\nBody text.\n");
        let kids = doc.children();
        assert_eq!(kids.len(), 2);
        assert!(matches!(kids[0], Node::Heading { level: 1, .. }));
        assert!(matches!(kids[1], Node::Paragraph(_)));
    }

    #[test]
    fn folds_soft_breaks_into_text() {
        let doc = parse("a\nb\n");
        let para = &doc.children()[0];
        let text: String = para.children().iter().map(|n| n.text_content()).collect();
        assert_eq!(text, "a\nb");
    }

    #[test]
    fn parses_task_list_markers() {
        let doc = parse("- [x] done\n- [ ] todo\n");
        let list = &doc.children()[0];
        let items = list.children();
        assert!(matches!(
            items[0],
            Node::ListItem {
                task: Some(true),
                ..
            }
        ));
        assert!(matches!(
            items[1],
            Node::ListItem {
                task: Some(false),
                ..
            }
        ));
    }

    #[test]
    fn parses_tables_with_alignments() {
        let doc = parse("| a | b |\n|:--|--:|\n| 1 | 2 |\n");
        match &doc.children()[0] {
            Node::Table {
                alignments,
                head,
                rows,
            } => {
                assert_eq!(alignments, &[Alignment::Left, Alignment::Right]);
                assert_eq!(head.len(), 2);
                assert_eq!(rows.len(), 1);
            }
            other => panic!("expected a table, got {other:?}"),
        }
    }

    #[test]
    fn distinguishes_autolinks_from_links() {
        let doc = parse("<https://example.com> and [x](https://y.example)\n");
        let kids = doc.children()[0].children();
        assert!(matches!(
            kids[0],
            Node::AutoLink {
                is_email: false,
                ..
            }
        ));
        assert!(kids.iter().any(|n| matches!(n, Node::Link { .. })));
    }

    #[test]
    fn linkifies_bare_urls() {
        let doc = parse("see https://example.com/x now\n");
        let kids = doc.children()[0].children();
        let link = kids
            .iter()
            .find_map(|n| match n {
                Node::AutoLink { url, .. } => Some(url.clone()),
                _ => None,
            })
            .expect("bare url should linkify");
        assert_eq!(link, "https://example.com/x");
    }

    #[test]
    fn bare_url_drops_trailing_period() {
        let doc = parse("go to https://example.com/x.\n");
        let kids = doc.children()[0].children();
        let link = kids
            .iter()
            .find_map(|n| match n {
                Node::AutoLink { url, .. } => Some(url.clone()),
                _ => None,
            })
            .expect("bare url should linkify");
        assert_eq!(link, "https://example.com/x");
    }

    #[test]
    fn code_blocks_keep_their_language() {
        let doc = parse("```go\npackage main\n```\n");
        match &doc.children()[0] {
            Node::CodeBlock { code, language } => {
                assert_eq!(language, "go");
                assert_eq!(code, "package main\n");
            }
            other => panic!("expected a code block, got {other:?}"),
        }
    }
}
