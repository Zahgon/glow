//! Resolving the user's editor.
//!
//! Reimplements `github.com/charmbracelet/x/editor`: the `$EDITOR` lookup with
//! its `nano` fallback, and the per-editor line-number options.

use std::process::Command;

/// An editor invocation option.
///
/// Each option is asked, for a given editor name and file path, which extra
/// arguments it contributes and whether it has already placed the path among
/// them.
pub enum Opt {
    /// Open the file at a line number, where the editor supports it.
    LineNumber(u64),
}

const DEFAULT_EDITOR: &str = "nano";
const PLUS_LINE_EDITORS: [&str; 7] = ["vi", "vim", "nvim", "nano", "emacs", "kak", "gedit"];

impl Opt {
    fn apply(&self, editor: &str, filename: &str) -> (Vec<String>, bool) {
        match self {
            Opt::LineNumber(n) => {
                if PLUS_LINE_EDITORS.contains(&editor) {
                    return (vec![format!("+{n}")], false);
                }
                if editor == "code" {
                    return (vec!["--goto".into(), format!("{filename}:{n}")], true);
                }
                (Vec::new(), false)
            }
        }
    }
}

/// The program and arguments that edit `path`.
///
/// Returned separately from the [`Command`] so callers can inspect them; use
/// [`cmd`] to get something runnable.
pub fn command_line(app: &str, path: &str, options: &[Opt]) -> Result<Vec<String>, String> {
    if std::env::var("SNAP_REVISION").map(|v| !v.is_empty()) == Ok(true) {
        return Err(format!(
            "Did you install with Snap? {app} is sandboxed and unable to open an editor. Please install {app} with Go or another package manager to enable editing."
        ));
    }

    let (editor, mut args) = get_editor();
    let editor_name = base_name(&editor);

    let mut needs_path = true;
    for opt in options {
        let (opt_args, path_in_args) = opt.apply(&editor_name, path);
        if path_in_args {
            needs_path = false;
        }
        args.extend(opt_args);
    }
    if needs_path {
        args.push(path.to_string());
    }

    let mut line = vec![editor];
    line.extend(args);
    Ok(line)
}

/// A [`Command`] editing `path`.
pub fn cmd(app: &str, path: &str, options: &[Opt]) -> Result<Command, String> {
    let line = command_line(app, path, options)?;
    let mut c = Command::new(&line[0]);
    c.args(&line[1..]);
    Ok(c)
}

/// `$EDITOR` split into a program and its arguments, or `nano`.
fn get_editor() -> (String, Vec<String>) {
    let raw = std::env::var("EDITOR").unwrap_or_default();
    let mut parts = raw.split_whitespace().map(str::to_string);
    match parts.next() {
        Some(first) => (first, parts.collect()),
        None => (DEFAULT_EDITOR.to_string(), Vec::new()),
    }
}

/// `filepath.Base` restricted to what an editor path needs.
fn base_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The environment is process-global, so the cases that set `EDITOR` share
    /// one test to keep them ordered.
    #[test]
    fn resolves_the_editor_and_line_number_options() {
        let saved = std::env::var("EDITOR").ok();

        std::env::remove_var("EDITOR");
        assert_eq!(
            command_line("Glow", "/tmp/a.md", &[]).unwrap(),
            vec!["nano", "/tmp/a.md"]
        );
        assert_eq!(
            command_line("Glow", "/tmp/a.md", &[Opt::LineNumber(12)]).unwrap(),
            vec!["nano", "+12", "/tmp/a.md"]
        );

        std::env::set_var("EDITOR", "/usr/bin/vim -u NONE");
        assert_eq!(
            command_line("Glow", "/tmp/a.md", &[Opt::LineNumber(3)]).unwrap(),
            vec!["/usr/bin/vim", "-u", "NONE", "+3", "/tmp/a.md"]
        );

        std::env::set_var("EDITOR", "code");
        assert_eq!(
            command_line("Glow", "/tmp/a.md", &[Opt::LineNumber(7)]).unwrap(),
            vec!["code", "--goto", "/tmp/a.md:7"]
        );

        std::env::set_var("EDITOR", "subl");
        assert_eq!(
            command_line("Glow", "/tmp/a.md", &[Opt::LineNumber(7)]).unwrap(),
            vec!["subl", "/tmp/a.md"],
            "an unsupported editor ignores the line number"
        );

        match saved {
            Some(v) => std::env::set_var("EDITOR", v),
            None => std::env::remove_var("EDITOR"),
        }
    }
}
