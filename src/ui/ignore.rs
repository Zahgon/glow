//! Directories the document search skips.

use super::config::Config;

/// The patterns excluded from the search unless `--all` is given.
#[cfg(target_os = "macos")]
pub fn ignore_patterns(cfg: &Config) -> Vec<String> {
    vec![
        format!("{}/Library", cfg.home_dir.trim_end_matches('/')),
        cfg.gopath.clone(),
        "node_modules".to_string(),
        ".*".to_string(),
    ]
}

/// The patterns excluded from the search unless `--all` is given.
#[cfg(not(target_os = "macos"))]
pub fn ignore_patterns(cfg: &Config) -> Vec<String> {
    vec![
        cfg.gopath.clone(),
        "node_modules".to_string(),
        ".*".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_files_and_dependencies_are_skipped() {
        let cfg = Config {
            home_dir: "/Users/x".into(),
            gopath: "/Users/x/go".into(),
            ..Config::default()
        };
        let patterns = ignore_patterns(&cfg);
        assert!(patterns.contains(&"node_modules".to_string()));
        assert!(patterns.contains(&".*".to_string()));
        assert!(patterns.contains(&"/Users/x/go".to_string()));
    }
}
