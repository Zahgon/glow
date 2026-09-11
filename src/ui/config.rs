//! TUI-specific configuration.

/// Configuration for the TUI, assembled from the environment and the resolved
/// command-line options.
#[derive(Debug, Clone)]
pub struct Config {
    /// List system files and directories.
    pub show_all_files: bool,
    /// Number every line in the pager.
    pub show_line_numbers: bool,
    /// `$GOPATH`, skipped while walking for documents.
    pub gopath: String,
    /// `$HOME`, used to skip `~/Library` on macOS.
    pub home_dir: String,
    /// Upper bound on the render width.
    pub glamour_max_width: u64,
    /// `$GLAMOUR_STYLE`, when the environment names one.
    pub glamour_style: String,
    /// Track the mouse wheel.
    pub enable_mouse: bool,
    /// Keep soft line breaks when rendering.
    pub preserve_new_lines: bool,
    /// Working directory or file path.
    pub path: String,
    /// Render through Glamour at all; `$GLOW_ENABLE_GLAMOUR` can turn it off
    /// while debugging the UI.
    pub glamour_enabled: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            show_all_files: false,
            show_line_numbers: false,
            gopath: String::new(),
            home_dir: String::new(),
            glamour_max_width: 0,
            glamour_style: String::new(),
            enable_mouse: false,
            preserve_new_lines: false,
            path: String::new(),
            glamour_enabled: true,
        }
    }
}

impl Config {
    /// Reads the environment-backed fields, as `env.ParseAs[ui.Config]` does.
    pub fn from_env() -> Result<Config, String> {
        Ok(Config {
            gopath: std::env::var("GOPATH").unwrap_or_default(),
            home_dir: std::env::var("HOME").unwrap_or_default(),
            glamour_style: std::env::var("GLAMOUR_STYLE").unwrap_or_default(),
            glamour_enabled: parse_bool_env("GLOW_ENABLE_GLAMOUR", "GlamourEnabled", true)?,
            ..Config::default()
        })
    }
}

/// `env`'s boolean parsing, including the error text it reports.
fn parse_bool_env(key: &str, field: &str, default: bool) -> Result<bool, String> {
    let raw = match std::env::var(key) {
        Ok(v) if !v.is_empty() => v,
        _ => return Ok(default),
    };
    match raw.as_str() {
        "1" | "t" | "T" | "true" | "TRUE" | "True" => Ok(true),
        "0" | "f" | "F" | "false" | "FALSE" | "False" => Ok(false),
        other => Err(format!(
            "env: parse error on field \"{field}\" of type \"bool\": strconv.ParseBool: parsing \"{other}\": invalid syntax"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glamour_is_enabled_unless_the_environment_says_otherwise() {
        let saved = std::env::var("GLOW_ENABLE_GLAMOUR").ok();

        std::env::remove_var("GLOW_ENABLE_GLAMOUR");
        assert!(Config::from_env().expect("parses").glamour_enabled);

        std::env::set_var("GLOW_ENABLE_GLAMOUR", "false");
        assert!(!Config::from_env().expect("parses").glamour_enabled);

        std::env::set_var("GLOW_ENABLE_GLAMOUR", "nope");
        assert_eq!(
            Config::from_env().unwrap_err(),
            "env: parse error on field \"GlamourEnabled\" of type \"bool\": strconv.ParseBool: parsing \"nope\": invalid syntax"
        );

        match saved {
            Some(v) => std::env::set_var("GLOW_ENABLE_GLAMOUR", v),
            None => std::env::remove_var("GLOW_ENABLE_GLAMOUR"),
        }
    }
}
