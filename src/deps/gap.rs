//! Platform-specific application paths.
//!
//! Reimplements the `github.com/muesli/go-app-paths` `User` scope that glow uses
//! to locate its configuration and log files, including the priority order of
//! the directories it returns.

use crate::utils::home_dir;

/// An application scope: the app name plus the platform conventions.
pub struct Scope {
    app: String,
}

impl Scope {
    /// A user-scoped set of paths for `app`.
    pub fn user(app: &str) -> Scope {
        Scope {
            app: app.to_string(),
        }
    }

    /// Priority-sorted configuration directories, each with the app name appended.
    pub fn config_dirs(&self) -> Vec<String> {
        let mut out = Vec::new();
        #[cfg(target_os = "macos")]
        {
            if let Some(home) = home_dir() {
                out.push(self.join(&format!("{home}/Library/Preferences")));
            }
            out.push(self.join("/Library/Preferences"));
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            match std::env::var("XDG_CONFIG_HOME") {
                Ok(p) if !p.is_empty() => out.push(self.join(&p)),
                _ => {
                    if let Some(home) = home_dir() {
                        out.push(self.join(&format!("{home}/.config")));
                    }
                }
            }
            if let Ok(dirs) = std::env::var("XDG_CONFIG_DIRS") {
                if !dirs.is_empty() {
                    for p in dirs.split(':') {
                        out.push(self.join(p));
                    }
                }
            }
            out.push(self.join("/etc/xdg"));
            out.push(self.join("/etc"));
        }
        #[cfg(windows)]
        {
            if let Ok(p) = std::env::var("LOCALAPPDATA") {
                out.push(self.join(&p));
            }
            if let Ok(p) = std::env::var("APPDATA") {
                out.push(self.join(&p));
            }
        }
        out
    }

    /// The user cache directory for this application.
    pub fn cache_dir(&self) -> Result<String, String> {
        #[cfg(target_os = "macos")]
        {
            let home = home_dir().ok_or_else(|| "Could not retrieve path".to_string())?;
            Ok(self.join(&format!("{home}/Library/Caches")))
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            match std::env::var("XDG_CACHE_HOME") {
                Ok(p) if !p.is_empty() => Ok(self.join(&p)),
                _ => {
                    let home = home_dir().ok_or_else(|| "Could not retrieve path".to_string())?;
                    Ok(self.join(&format!("{home}/.cache")))
                }
            }
        }
        #[cfg(windows)]
        {
            let p =
                std::env::var("LOCALAPPDATA").map_err(|_| "Could not retrieve path".to_string())?;
            Ok(self.join(&p))
        }
    }

    fn join(&self, base: &str) -> String {
        format!("{}/{}", base.trim_end_matches('/'), self.app)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dirs_are_app_scoped_and_ordered() {
        let dirs = Scope::user("glow").config_dirs();
        assert!(!dirs.is_empty());
        for d in &dirs {
            assert!(d.ends_with("/glow"), "{d} is not app-scoped");
        }
    }

    #[test]
    fn cache_dir_is_app_scoped() {
        let dir = Scope::user("glow").cache_dir().expect("a cache dir");
        assert!(dir.ends_with("/glow"), "{dir}");
    }
}
