//! The `config` sub-command: create the configuration file and open an editor.

use std::io::Write;
use std::path::Path;

use crate::deps::cobra::{Args, Command, Flag};
use crate::deps::editor;
use crate::deps::go_exec_error;
use crate::style::{keyword, paragraph};
use crate::utils::extension;

/// The configuration file written when none exists yet.
pub const DEFAULT_CONFIG: &str = concat!(
    "# style name or JSON path (default \"auto\")\n",
    "style: \"auto\"\n",
    "# mouse support (TUI-mode only)\n",
    "mouse: false\n",
    "# use pager to display markdown\n",
    "pager: false\n",
    "# word-wrap at width\n",
    "width: 80\n",
    "# show all files, including hidden and ignored.\n",
    "all: false\n",
);

/// The `config` command definition.
pub fn command() -> Command {
    let mut cmd = Command::new("config", "config", "Edit the glow config file");
    cmd.long = paragraph(&format!(
        "\n{} the glow config file. We\u{2019}ll use EDITOR to determine which editor to use. If the config file doesn't exist, it will be created.",
        keyword("Edit")
    ));
    cmd.example = paragraph("glow config\nglow config --config path/to/config.yml");
    cmd.args = Args::NoArgs;
    cmd.flags
        .add(Flag::bool("help", "h", false, "help for config"));
    cmd
}

/// Runs the command: make sure the file exists, edit it, and report where it is.
pub fn run(config_file: &mut String, config_file_used: &str) -> Result<(), String> {
    ensure_config_file(config_file, config_file_used)?;

    let line = editor::command_line("Glow", config_file, &[])?;
    let status = std::process::Command::new(&line[0])
        .args(&line[1..])
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| format!("unable to run command: {}", go_exec_error(&line[0], &e)))?;
    if !status.success() {
        return Err(format!(
            "unable to run command: exit status {}",
            status.code().unwrap_or(1)
        ));
    }

    println!("Wrote config file to: {config_file}");
    Ok(())
}

/// Creates the configuration file, and every directory leading to it, unless it
/// is already there.
///
/// `config_file` is the `--config` value, which may be empty; when it is, the
/// path of the configuration file the search actually read is used instead.
pub fn ensure_config_file(config_file: &mut String, config_file_used: &str) -> Result<(), String> {
    if config_file.is_empty() {
        *config_file = config_file_used.to_string();
        std::fs::create_dir_all(parent_of(config_file))
            .map_err(|e| format!("could not write configuration file: {e}"))?;
    }

    let ext = extension(config_file);
    if ext != ".yaml" && ext != ".yml" {
        return Err(format!(
            "'{ext}' is not a supported configuration type: use '.yaml' or '.yml'"
        ));
    }

    match std::fs::metadata(&*config_file) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(parent_of(config_file))
                .map_err(|e| format!("unable create directory: {e}"))?;
            let mut f = std::fs::File::create(&*config_file)
                .map_err(|e| format!("unable to create config file: {e}"))?;
            f.write_all(DEFAULT_CONFIG.as_bytes())
                .map_err(|e| format!("unable to write config file: {e}"))
        }
        Err(e) => Err(format!("unable to stat config file: {e}")),
    }
}

/// `filepath.Dir`, which answers `.` for a bare name and for the empty string.
fn parent_of(path: &str) -> String {
    match Path::new(path).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_string_lossy().into_owned(),
        _ => ".".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_created_with_the_default_config() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let mut path = dir
            .path()
            .join("nested")
            .join("glow.yml")
            .to_string_lossy()
            .into_owned();
        ensure_config_file(&mut path, "").expect("creates the file");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DEFAULT_CONFIG);
    }

    #[test]
    fn an_existing_file_is_left_alone() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let mut path = dir.path().join("glow.yaml").to_string_lossy().into_owned();
        std::fs::write(&path, "style: dark\n").expect("write");
        ensure_config_file(&mut path, "").expect("leaves the file");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "style: dark\n");
    }

    #[test]
    fn an_unsupported_extension_is_rejected() {
        let mut path = "/tmp/glow.txt".to_string();
        assert_eq!(
            ensure_config_file(&mut path, "").unwrap_err(),
            "'.txt' is not a supported configuration type: use '.yaml' or '.yml'"
        );
        let mut empty = String::new();
        assert_eq!(
            ensure_config_file(&mut empty, "").unwrap_err(),
            "'' is not a supported configuration type: use '.yaml' or '.yml'"
        );
    }

    #[test]
    fn the_default_config_matches_the_original_byte_for_byte() {
        assert_eq!(DEFAULT_CONFIG.lines().count(), 10);
        assert!(DEFAULT_CONFIG.starts_with("# style name or JSON path (default \"auto\")\n"));
        assert!(DEFAULT_CONFIG.ends_with("all: false\n"));
    }

    #[test]
    fn the_example_block_is_paragraph_styled() {
        let cmd = command();
        assert_eq!(
            cmd.example,
            paragraph("glow config\nglow config --config path/to/config.yml")
        );
        assert!(cmd.example.starts_with("  glow config"));
    }
}
