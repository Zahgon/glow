//! Utility functions.

use std::path::Path;

use crate::deps::glamour::style::StyleConfig;
use crate::deps::glamour::styles;
use crate::deps::glamour::StyleSource;
use crate::deps::lipgloss;

/// Removes the front matter header of a markdown file.
pub fn remove_frontmatter(content: &[u8]) -> &[u8] {
    let boundaries = detect_frontmatter(content);
    match boundaries {
        Some((0, end)) => &content[end..],
        _ => content,
    }
}

/// Finds the byte range covered by a YAML front matter header.
///
/// Equivalent to matching `(?m)^---\r?\n(\s*\r?\n)?` twice and returning the
/// start of the first match and the end of the second.
fn detect_frontmatter(c: &[u8]) -> Option<(usize, usize)> {
    let mut matches = Vec::new();
    let mut i = 0usize;
    while i < c.len() && matches.len() < 2 {
        if (i == 0 || c[i - 1] == b'\n') && c[i..].starts_with(b"---") {
            let mut j = i + 3;
            if c.get(j) == Some(&b'\r') {
                j += 1;
            }
            if c.get(j) == Some(&b'\n') {
                j += 1;
                // The optional `(\s*\r?\n)?` group: blank space up to and
                // including one more newline.
                let mut k = j;
                while k < c.len() && (c[k] as char).is_whitespace() && c[k] != b'\n' {
                    k += 1;
                }
                if c.get(k) == Some(&b'\r') {
                    k += 1;
                }
                if c.get(k) == Some(&b'\n') {
                    j = k + 1;
                }
                matches.push((i, j));
                i = j;
                continue;
            }
        }
        i += 1;
    }
    if matches.len() > 1 {
        Some((matches[0].0, matches[1].1))
    } else {
        None
    }
}

/// Expands a leading tilde and all environment variables in `path`.
pub fn expand_path(path: &str) -> String {
    let expanded = expand_home(path);
    expand_env(&expanded)
}

fn expand_home(path: &str) -> String {
    if path == "~" {
        return home_dir().unwrap_or_else(|| path.to_string());
    }
    match path.strip_prefix("~/") {
        Some(rest) => match home_dir() {
            Some(home) => format!("{}/{}", home.trim_end_matches('/'), rest),
            None => path.to_string(),
        },
        None => path.to_string(),
    }
}

/// The current user's home directory, as `go-homedir` resolves it.
pub fn home_dir() -> Option<String> {
    std::env::var("HOME").ok().filter(|h| !h.is_empty())
}

/// Expands `$VAR` and `${VAR}` references, as `os.ExpandEnv` does.
fn expand_env(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'{' {
                if let Some(end) = s[i + 2..].find('}') {
                    let name = &s[i + 2..i + 2 + end];
                    out.push_str(&std::env::var(name).unwrap_or_default());
                    i = i + 2 + end + 1;
                    continue;
                }
            }
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end > start {
                out.push_str(&std::env::var(&s[start..end]).unwrap_or_default());
                i = end;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Wraps a string in a fenced code block tagged with `language`.
pub fn wrap_code_block(s: &str, language: &str) -> String {
    format!("```{language}\n{s}```")
}

const MARKDOWN_EXTENSIONS: [&str; 5] = [".md", ".mdown", ".mkdn", ".mkd", ".markdown"];

/// Whether the filename has a markdown extension.
///
/// A name with no extension is assumed to be markdown; anything else with an
/// extension that is not in the list is treated as code.
pub fn is_markdown_file(filename: &str) -> bool {
    let ext = extension(filename);
    if ext.is_empty() {
        return true;
    }
    MARKDOWN_EXTENSIONS
        .iter()
        .any(|v| v.eq_ignore_ascii_case(&ext))
}

/// `filepath.Ext`: the suffix from the final dot of the last path element.
pub fn extension(path: &str) -> String {
    let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match base.rfind('.') {
        Some(i) => base[i..].to_string(),
        None => String::new(),
    }
}

/// Resolves `style` into a Glamour stylesheet for the given kind of source.
///
/// For a pure code block the code-block margin is removed, so the fenced block
/// is not indented twice.
pub fn glamour_style(style: &str, is_code: bool) -> StyleSource {
    if !is_code {
        if style == "auto" {
            return StyleSource::Standard(styles::DARK_STYLE.into());
        }
        return StyleSource::Path(style.into());
    }

    let config: StyleConfig = match style {
        "auto" => {
            if lipgloss::has_dark_background() {
                styles::default_style(styles::DARK_STYLE)
            } else {
                styles::default_style(styles::LIGHT_STYLE)
            }
        }
        styles::DARK_STYLE => styles::default_style(styles::DARK_STYLE),
        styles::LIGHT_STYLE => styles::default_style(styles::LIGHT_STYLE),
        styles::PINK_STYLE => styles::default_style(styles::PINK_STYLE),
        styles::NOTTY_STYLE => styles::default_style(styles::NOTTY_STYLE),
        styles::DRACULA_STYLE => styles::default_style(styles::DRACULA_STYLE),
        // Upstream maps tokyo-night to the Dracula config for code blocks.
        styles::TOKYO_NIGHT_STYLE => styles::default_style(styles::DRACULA_STYLE),
        _ => return StyleSource::JsonFile(style.into()),
    }
    .expect("built-in style parses");

    let mut config = config;
    config.code_block.block.margin = Some(0);
    StyleSource::Config(Box::new(config))
}

/// Whether `path` names an existing directory.
pub fn is_dir(path: &str) -> bool {
    Path::new(path).is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_frontmatter_only_at_the_start() {
        let src = b"---\ntitle: x\n---\n\n# H\n";
        assert_eq!(remove_frontmatter(src), b"# H\n");
        let no_fm = b"# H\n\n---\n\nbody\n";
        assert_eq!(remove_frontmatter(no_fm), no_fm);
    }

    #[test]
    fn keeps_content_without_two_delimiters() {
        let src = b"---\ntitle: x\n";
        assert_eq!(remove_frontmatter(src), src);
    }

    #[test]
    fn wraps_code_blocks_with_the_language_tag() {
        assert_eq!(wrap_code_block("x\n", ".rs"), "```.rs\nx\n```");
    }

    #[test]
    fn markdown_extensions_are_case_insensitive() {
        for name in ["a.md", "a.MD", "a.markdown", "a.Mkd", "a.mdown", "a.mkdn"] {
            assert!(is_markdown_file(name), "{name}");
        }
        for name in ["a.rs", "a.go", "a.txt", "a.png"] {
            assert!(!is_markdown_file(name), "{name}");
        }
        assert!(is_markdown_file("README"), "no extension means markdown");
    }

    #[test]
    fn extension_takes_the_last_dot_of_the_base_name() {
        assert_eq!(extension("/a.b/c.rs"), ".rs");
        assert_eq!(extension("/a.b/c"), "");
    }

    #[test]
    fn expands_environment_variables() {
        std::env::set_var("GLOW_TEST_VAR", "value");
        assert_eq!(expand_env("x/$GLOW_TEST_VAR/y"), "x/value/y");
        assert_eq!(expand_env("x/${GLOW_TEST_VAR}/y"), "x/value/y");
        std::env::remove_var("GLOW_TEST_VAR");
    }

    #[test]
    fn glamour_style_maps_auto_to_dark_for_prose() {
        match glamour_style("auto", false) {
            StyleSource::Standard(name) => assert_eq!(name, "dark"),
            other => panic!("expected the dark standard style, got {other:?}"),
        }
    }

    #[test]
    fn glamour_style_removes_the_code_block_margin_for_code() {
        match glamour_style("notty", true) {
            StyleSource::Config(c) => assert_eq!(c.code_block.block.margin, Some(0)),
            other => panic!("expected an inline config, got {other:?}"),
        }
    }

    #[test]
    fn glamour_style_falls_back_to_a_json_file_for_code() {
        match glamour_style("/tmp/custom.json", true) {
            StyleSource::JsonFile(p) => assert_eq!(p, "/tmp/custom.json"),
            other => panic!("expected a JSON file source, got {other:?}"),
        }
    }
}
