//! Markdown rendering for the terminal.
//!
//! A reimplementation of `charm.land/glamour/v2`: the style model, the built-in
//! stylesheets, and the ANSI renderer that turns a CommonMark+GFM document into
//! styled, wrapped, margined terminal text.

pub mod ast;
pub mod autolink;
pub mod render;
pub mod style;
pub mod styles;
pub mod table;

use std::path::Path;

pub use render::{Options, Renderer};
pub use style::StyleConfig;

/// Failure to build a renderer.
#[derive(Debug)]
pub enum Error {
    /// The named style is neither built in nor a readable file.
    StyleNotFound(String),
    /// The stylesheet could not be read, at this path.
    Io(String, std::io::Error),
    /// The stylesheet was not valid JSON.
    Json(serde_json::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::StyleNotFound(s) => write!(f, "{s}: style not found"),
            Error::Io(path, e) => write!(
                f,
                "glamour: error reading file: open {path}: {}",
                super::go_errno(e)
            ),
            Error::Json(e) => write!(f, "glamour: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// How a renderer should obtain its stylesheet.
#[derive(Debug, Clone)]
pub enum StyleSource {
    /// One of the built-in styles, by name.
    Standard(String),
    /// A path that is first tried as a built-in name, then as a JSON file.
    Path(String),
    /// A JSON stylesheet file.
    JsonFile(String),
    /// An already-built stylesheet.
    Config(Box<StyleConfig>),
}

/// Resolves a [`StyleSource`] into a stylesheet.
pub fn resolve_style(source: &StyleSource) -> Result<StyleConfig, Error> {
    match source {
        StyleSource::Standard(name) => {
            styles::default_style(name).ok_or_else(|| Error::StyleNotFound(name.clone()))
        }
        StyleSource::Path(path) => match styles::default_style(path) {
            Some(s) => Ok(s),
            None => read_json_style(path),
        },
        StyleSource::JsonFile(path) => read_json_style(path),
        StyleSource::Config(c) => Ok((**c).clone()),
    }
}

fn read_json_style(path: &str) -> Result<StyleConfig, Error> {
    let bytes = std::fs::read(Path::new(path)).map_err(|e| Error::Io(path.to_string(), e))?;
    serde_json::from_slice(&bytes).map_err(Error::Json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_built_in_name_resolves_without_touching_the_disk() {
        let cfg = resolve_style(&StyleSource::Standard(styles::NOTTY_STYLE.into()))
            .expect("a built-in style");
        assert_eq!(cfg.document.margin, Some(2));
    }

    #[test]
    fn a_path_tries_the_built_in_names_first() {
        let cfg =
            resolve_style(&StyleSource::Path(styles::DARK_STYLE.into())).expect("a built-in style");
        assert_eq!(cfg.document.primitive.color.as_deref(), Some("252"));
    }

    #[test]
    fn a_missing_stylesheet_reports_gos_open_error() {
        let err = resolve_style(&StyleSource::JsonFile("ascii".into())).expect_err("no such file");
        assert_eq!(
            err.to_string(),
            "glamour: error reading file: open ascii: no such file or directory"
        );
    }

    #[test]
    fn an_unknown_standard_name_is_named_in_the_error() {
        let err =
            resolve_style(&StyleSource::Standard("nope".into())).expect_err("not a built-in style");
        assert_eq!(err.to_string(), "nope: style not found");
    }

    #[test]
    fn a_malformed_stylesheet_reports_the_json_error() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "{ not json").expect("write");
        let err = resolve_style(&StyleSource::JsonFile(path.to_string_lossy().into()))
            .expect_err("malformed");
        assert!(err.to_string().starts_with("glamour: "), "{err}");
    }
}
