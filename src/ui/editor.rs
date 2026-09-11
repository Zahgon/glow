//! Opening a document in `$EDITOR`.

use crate::deps::bubbletea::Cmd;
use crate::deps::editor;

use super::Msg;

/// A command that suspends the TUI and edits `path` at `lineno`.
pub fn open_editor(path: &str, lineno: u64) -> Cmd<Msg> {
    match editor::command_line("Glow", path, &[editor::Opt::LineNumber(lineno)]) {
        Ok(argv) => Cmd::Exec(argv, Box::new(Msg::EditorFinished)),
        Err(e) => {
            let e = e.clone();
            Cmd::Async(Box::new(move || Some(Msg::EditorFinished(Some(e)))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_carries_the_path_and_line() {
        let saved = std::env::var("EDITOR").ok();
        std::env::set_var("EDITOR", "vim");
        match open_editor("/tmp/a.md", 12) {
            Cmd::Exec(argv, _) => assert_eq!(argv, vec!["vim", "+12", "/tmp/a.md"]),
            _ => panic!("expected an exec command"),
        }
        match saved {
            Some(v) => std::env::set_var("EDITOR", v),
            None => std::env::remove_var("EDITOR"),
        }
    }
}
