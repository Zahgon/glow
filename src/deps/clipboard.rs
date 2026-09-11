//! Copying to the clipboard.
//!
//! Reimplements the two mechanisms glow uses together: `termenv.Copy`, which
//! writes an OSC 52 sequence to the terminal, and `atotto/clipboard`, which
//! shells out to the platform's clipboard tool.

use std::io::Write;

/// Writes `s` to the terminal's clipboard with OSC 52.
pub fn osc52_copy(s: &str) {
    let encoded = base64(s.as_bytes());
    let mut out = std::io::stdout();
    let _ = write!(out, "\u{1b}]52;c;{encoded}\u{7}");
    let _ = out.flush();
}

/// Writes `s` to the system clipboard, if a tool for it exists.
pub fn write_all(s: &str) -> Result<(), String> {
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-in", "-selection", "clipboard"]),
            ("xsel", &["--input", "--clipboard"]),
        ]
    };

    let mut last = "no clipboard utilities available".to_string();
    for (program, args) in candidates {
        let child = std::process::Command::new(program)
            .args(*args)
            .stdin(std::process::Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                last = super::go_exec_error(program, &e);
                continue;
            }
        };
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(s.as_bytes());
        }
        drop(child.stdin.take());
        return child.wait().map(|_| ()).map_err(|e| e.to_string());
    }
    Err(last)
}

/// Standard base64, which is what OSC 52 payloads use.
fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads_partial_groups() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
