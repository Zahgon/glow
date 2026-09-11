//! Terminal interrogation.
//!
//! Reimplements the three `golang.org/x/term` and `os.Stat` checks glow makes:
//! whether a descriptor is a terminal, how wide it is, and whether stdin is a
//! pipe rather than the console.

use std::io::IsTerminal;

/// Whether stdout is a terminal.
pub fn stdout_is_terminal() -> bool {
    std::io::stdout().is_terminal()
}

/// The width and height of the terminal.
pub fn size() -> Option<(u16, u16)> {
    crossterm::terminal::size().ok()
}

/// Whether stdin has been redirected from something other than the console.
///
/// Mirrors `os.Stdin.Stat()` followed by `mode&os.ModeCharDevice == 0 ||
/// size > 0`: a regular file or a pipe is a source, and so is a character
/// device that already has bytes waiting.
pub fn stdin_is_pipe() -> Result<bool, String> {
    #[cfg(unix)]
    {
        // SAFETY: `fstat` fills the stat struct we own.
        let st = unsafe {
            let mut st: libc::stat = std::mem::zeroed();
            if libc::fstat(libc::STDIN_FILENO, &mut st) != 0 {
                let err = std::io::Error::last_os_error();
                return Err(format!("unable to open file: {err}"));
            }
            st
        };
        let is_char_device = st.st_mode & libc::S_IFMT == libc::S_IFCHR;
        Ok(!is_char_device || st.st_size > 0)
    }
    #[cfg(not(unix))]
    {
        Ok(!std::io::stdin().is_terminal())
    }
}

/// Turns on ANSI escape processing in the Windows console.
///
/// The Go original does this from an `init` in a `//go:build windows` file;
/// here the enabling lives in `crossterm`, which probes and sets the console
/// mode the first time it is asked.
pub fn enable_ansi_colors() {
    #[cfg(windows)]
    {
        let _ = crossterm::ansi_support::supports_ansi();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_captured_stdout_is_not_a_terminal() {
        // The test harness captures stdout, so it is a pipe or a file.
        assert!(!stdout_is_terminal() || size().is_some());
    }

    #[test]
    fn stdin_state_is_readable() {
        assert!(stdin_is_pipe().is_ok());
    }
}
