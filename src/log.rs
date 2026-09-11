//! File logging.
//!
//! Glow logs to a file under the user cache directory and never to the
//! terminal. If the directory or the file cannot be opened, logging is silently
//! disabled — it is never fatal.
//!
//! Until `setup` runs, the logger is still in `charmbracelet/log`'s default
//! state: stderr, at info level. Configuration is loaded before that point, so
//! a warning about an unparseable configuration file does reach the terminal.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;

use crate::deps::gap::Scope;

/// Where log records go.
enum Sink {
    /// The default logger: stderr, at info level.
    Stderr,
    /// A log file, at debug level.
    File(Box<File>),
    /// Logging is off.
    Discard,
}

static SINK: Mutex<Sink> = Mutex::new(Sink::Stderr);

/// The full path of the log file.
pub fn log_file_path() -> Result<String, String> {
    let dir = Scope::user("glow")
        .cache_dir()
        .map_err(|e| format!("unable to get cache dir: {e}"))?;
    Ok(format!("{dir}/glow.log"))
}

/// Opens the log file, creating its directory if needed.
///
/// Returns the resolved path when logging is active. Failure to create or open
/// the file disables logging rather than reporting an error, exactly as the
/// original does.
pub fn setup() -> Result<Option<String>, String> {
    *SINK.lock().expect("log mutex") = Sink::Discard;
    let path = log_file_path()?;
    let dir = std::path::Path::new(&path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    if std::fs::create_dir_all(&dir).is_err() {
        return Ok(None);
    }
    let file = match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(f) => f,
        Err(_) => return Ok(None),
    };
    *SINK.lock().expect("log mutex") = Sink::File(Box::new(file));
    Ok(Some(path))
}

/// Closes the log file.
pub fn close() {
    *SINK.lock().expect("log mutex") = Sink::Discard;
}

fn record(level: &str, message: &str, fields: &[(&str, String)]) -> String {
    let mut line = format!("{level} {message}");
    for (k, v) in fields {
        line.push_str(&format!(" {k}={}", quote(v)));
    }
    line.push('\n');
    line
}

/// `charmbracelet/log` quotes a value only when it is not a bare word.
fn quote(v: &str) -> String {
    let plain = !v.is_empty()
        && v.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '@' | '+' | ':'));
    if plain {
        v.to_string()
    } else {
        format!("{v:?}")
    }
}

fn write(level: &str, message: &str, fields: &[(&str, String)]) {
    let mut guard = SINK.lock().expect("log mutex");
    match &mut *guard {
        Sink::Discard => {}
        Sink::File(file) => {
            let _ = file.write_all(record(level, message, fields).as_bytes());
        }
        Sink::Stderr => {
            // The default logger drops anything below info and stamps the time.
            if level == "DEBU" {
                return;
            }
            let stamp = chrono::Local::now().format("%Y/%m/%d %H:%M:%S");
            eprint!("{stamp} {}", record(level, message, fields));
        }
    }
}

/// Logs at debug level.
pub fn debug(message: &str, fields: &[(&str, String)]) {
    write("DEBU", message, fields);
}

/// Logs at info level.
pub fn info(message: &str, fields: &[(&str, String)]) {
    write("INFO", message, fields);
}

/// Logs at warning level.
pub fn warn(message: &str, fields: &[(&str, String)]) {
    write("WARN", message, fields);
}

/// Logs at error level.
pub fn error(message: &str, fields: &[(&str, String)]) {
    write("ERRO", message, fields);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_path_is_under_the_cache_dir() {
        let path = log_file_path().expect("a log path");
        assert!(path.ends_with("/glow/glow.log"), "{path}");
    }

    #[test]
    fn logging_without_a_file_writes_nowhere() {
        // R9: when the log file cannot be opened, logging is disabled rather
        // than fatal, and nothing reaches the terminal.
        close();
        let before = log_file_path().ok().and_then(|p| std::fs::metadata(p).ok());
        debug("nothing opens", &[("key", "value".into())]);
        error("nor does this", &[]);
        let after = log_file_path().ok().and_then(|p| std::fs::metadata(p).ok());
        assert_eq!(
            before.map(|m| m.len()),
            after.map(|m| m.len()),
            "a closed logger must not grow the log file"
        );
    }

    #[test]
    fn records_carry_their_fields() {
        assert_eq!(
            record(
                "WARN",
                "Could not parse configuration file",
                &[("err", "boom now".into())]
            ),
            "WARN Could not parse configuration file err=\"boom now\"\n"
        );
        assert_eq!(
            record(
                "DEBU",
                "Using configuration file",
                &[("path", "/x/glow.yml".into())]
            ),
            "DEBU Using configuration file path=/x/glow.yml\n"
        );
    }
}
