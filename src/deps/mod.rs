//! Reimplementations of the third-party behaviour the Go original depended on.
//!
//! Each module here reproduces the *observable* behaviour of one Go package —
//! its output bytes, its error text, its algorithm — rather than wrapping a
//! Rust crate whose semantics merely resemble it. See `truth.md` for the
//! per-dependency decisions.

pub mod ansi;
pub mod bubbles;
pub mod bubbletea;
pub mod clipboard;
pub mod cobra;
pub mod editor;
pub mod fuzzy;
pub mod gap;
pub mod gitcha;
pub mod glamour;
pub mod humanize;
pub mod lipgloss;
pub mod mango;
pub mod roff;
pub mod shell;
pub mod term;
pub mod url;
pub mod viper;

/// The message Go's `os` package prints for an I/O error.
///
/// Rust's `io::Error` renders the same conditions differently — "No such file
/// or directory (os error 2)" against Go's "no such file or directory" — and
/// those strings reach the user through glow's error texts.
pub fn go_errno(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => "no such file or directory".into(),
        std::io::ErrorKind::PermissionDenied => "permission denied".into(),
        std::io::ErrorKind::IsADirectory => "is a directory".into(),
        _ => {
            // `io::Error`'s Display appends the OS code in brackets.
            let s = e.to_string();
            match s.split_once(" (os error") {
                Some((head, _)) => head.to_ascii_lowercase(),
                None => s,
            }
        }
    }
}

/// The message Go's `os/exec` package prints when a command cannot start.
///
/// A bare name is looked up on `$PATH` before the fork, so it fails with
/// `exec:`; a name with a separator fails in the fork itself.
pub fn go_exec_error(program: &str, e: &std::io::Error) -> String {
    if program.contains('/') {
        return format!("fork/exec {program}: {}", go_errno(e));
    }
    match e.kind() {
        std::io::ErrorKind::NotFound => {
            format!("exec: {program:?}: executable file not found in $PATH")
        }
        std::io::ErrorKind::PermissionDenied => format!("exec: {program:?}: permission denied"),
        _ => format!("exec: {program:?}: {}", go_errno(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    #[test]
    fn errno_text_drops_rusts_os_error_suffix() {
        assert_eq!(
            go_errno(&Error::from(ErrorKind::NotFound)),
            "no such file or directory"
        );
        assert_eq!(
            go_errno(&Error::from(ErrorKind::PermissionDenied)),
            "permission denied"
        );
        // Anything else keeps its wording but loses the bracketed code, which
        // Go never prints.
        let other = Error::from_raw_os_error(6);
        assert!(
            !go_errno(&other).contains("os error"),
            "{}",
            go_errno(&other)
        );
        assert_eq!(go_errno(&other), go_errno(&other).to_lowercase());
    }

    #[test]
    fn a_bare_program_name_fails_the_path_lookup() {
        assert_eq!(
            go_exec_error("nosuchpager", &Error::from(ErrorKind::NotFound)),
            "exec: \"nosuchpager\": executable file not found in $PATH"
        );
        assert_eq!(
            go_exec_error("nosuchpager", &Error::from(ErrorKind::PermissionDenied)),
            "exec: \"nosuchpager\": permission denied"
        );
    }

    #[test]
    fn a_program_with_a_separator_fails_in_the_fork() {
        assert_eq!(
            go_exec_error("/opt/bin/pager", &Error::from(ErrorKind::NotFound)),
            "fork/exec /opt/bin/pager: no such file or directory"
        );
    }
}
