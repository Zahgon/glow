//! Glow — render markdown on the CLI, with pizzazz.
//!
//! The library crate exists so the binary's behaviour can be exercised by the
//! test suite; `main.rs` is a thin entry point over it.

pub mod config_cmd;
pub mod deps;
pub mod github;
pub mod gitlab;
pub mod glow;
pub mod http;
pub mod log;
pub mod man_cmd;
pub mod source;
pub mod style;
pub mod ui;
pub mod url;
pub mod utils;
