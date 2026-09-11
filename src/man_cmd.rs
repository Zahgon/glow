//! The `man` sub-command: write a roff man page to stdout.

use crate::deps::cobra::{App, Args, Command, Flag};
use crate::deps::mango;

/// The `man` command definition.
pub fn command() -> Command {
    let mut cmd = Command::new("man", "man", "Generates manpages");
    cmd.hidden = true;
    cmd.disable_flags_in_use_line = true;
    cmd.args = Args::NoArgs;
    cmd.flags
        .add(Flag::bool("help", "h", false, "help for man"));
    cmd
}

/// Renders the man page for `app` and writes it to stdout.
pub fn run(app: &App) -> Result<(), String> {
    print!("{}", build(app, &today()));
    Ok(())
}

/// The man page text for `app`, dated `date`.
pub fn build(app: &App, date: &str) -> String {
    mango::from_cobra(1, &app.root).build(date)
}

/// The build date, in the format roff's title heading wants.
fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glow::Glow;

    fn page() -> String {
        let dir = tempfile::tempdir().expect("a temp dir");
        std::env::set_var("GLOW_CONFIG_HOME", dir.path());
        let glow = Glow::new();
        std::env::remove_var("GLOW_CONFIG_HOME");
        build(&glow.app, "2024-01-02")
    }

    #[test]
    fn the_heading_names_the_program_and_its_section() {
        assert!(page().starts_with(
            ".TH GLOW 1 \"2024-01-02\" \"glow\" \"Render markdown on the CLI, with pizzazz!\"\n"
        ));
    }

    #[test]
    fn the_synopsis_is_the_standard_one() {
        assert!(page().contains(
            "\n.SH SYNOPSIS\n\\fBglow\\fP [\\fIoptions\\&.\\&.\\&.\\fP] [\\fIargument\\&.\\&.\\&.\\fP]\n"
        ));
    }

    #[test]
    fn every_flag_is_listed_including_the_hidden_one() {
        let p = page();
        for flag in [
            "\\fB-a, --all\\fP",
            "\\fB--config\\fP",
            "\\fB-l, --line-numbers\\fP",
            "\\fB-m, --mouse\\fP",
            "\\fB-p, --pager\\fP",
            "\\fB-n, --preserve-new-lines\\fP",
            "\\fB-s, --style\\fP",
            "\\fB-t, --tui\\fP",
            "\\fB-w, --width\\fP",
        ] {
            assert!(p.contains(flag), "missing {flag}");
        }
        assert!(
            !p.contains("\\fB-h, --help\\fP"),
            "cobra adds help at run time"
        );
        assert!(
            !p.contains("\\fB-v, --version\\fP"),
            "cobra adds version at run time"
        );
    }

    #[test]
    fn hidden_commands_are_absent_and_nested_ones_are_present() {
        let p = page();
        assert!(p.contains("\\fBcompletion\\fP"));
        assert!(p.contains("\\fBconfig\\fP"));
        assert!(p.contains("\\fBhelp\\fP [command]"));
        assert!(!p.contains("\\fBman\\fP"), "man is hidden");
        assert!(p.contains("\\fBbash\\fP"));
        assert!(p.contains("\\fB--no-descriptions\\fP"));
    }

    #[test]
    fn the_config_example_block_is_rendered() {
        let p = page();
        assert!(p.contains("\\fBEXAMPLES\\fP"));
        assert!(p.contains("glow config --config path/to/config\\&.yml"));
    }

    #[test]
    fn the_page_ends_with_the_help_command() {
        assert!(page().ends_with("\\fBhelp\\fP [command]\n.RS 4\nHelp about any command\n.RE\n"));
    }
}
