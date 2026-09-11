//! POSIX shell field splitting.
//!
//! Reimplements the part of `mvdan.cc/sh/v3/shell` that glow uses: splitting a
//! `$PAGER` command into fields, honouring single quotes, double quotes,
//! backslash escapes and parameter expansion, and field-splitting the result of
//! an unquoted expansion on `IFS` whitespace.

/// Why a command line could not be split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError(pub String);

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SyntaxError {}

/// Splits `s` into fields, expanding variables through `env`.
///
/// `env` returns the value of a variable, or `None` when it is unset — the
/// same contract as the `func(string) string` the Go version takes, where an
/// unset variable expands to the empty string.
// `started` is reset inside a macro that expands at several points; the lint
// only sees the expansion where the reset is not read again.
#[allow(unused_assignments)]
pub fn fields<F>(s: &str, env: F) -> Result<Vec<String>, SyntaxError>
where
    F: Fn(&str) -> Option<String>,
{
    let mut out: Vec<String> = Vec::new();
    // The field being built, and whether anything at all has been written to
    // it — an empty quoted string is still a field.
    let mut cur = String::new();
    let mut started = false;

    let chars: Vec<char> = s.chars().collect();
    let mut i = 0usize;

    macro_rules! flush {
        () => {
            if started {
                out.push(std::mem::take(&mut cur));
                started = false;
            }
        };
    }

    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\n' => {
                flush!();
                i += 1;
            }
            '\'' => {
                started = true;
                i += 1;
                loop {
                    match chars.get(i) {
                        None => {
                            return Err(SyntaxError("reached EOF without closing quote '".into()))
                        }
                        Some('\'') => {
                            i += 1;
                            break;
                        }
                        Some(&other) => {
                            cur.push(other);
                            i += 1;
                        }
                    }
                }
            }
            '"' => {
                started = true;
                i += 1;
                loop {
                    match chars.get(i) {
                        None => {
                            return Err(SyntaxError("reached EOF without closing quote \"".into()))
                        }
                        Some('"') => {
                            i += 1;
                            break;
                        }
                        Some('\\') => {
                            // Inside double quotes a backslash only escapes
                            // these four characters; otherwise it is literal.
                            match chars.get(i + 1) {
                                Some(&n @ ('$' | '`' | '"' | '\\')) => {
                                    cur.push(n);
                                    i += 2;
                                }
                                Some('\n') => i += 2,
                                _ => {
                                    cur.push('\\');
                                    i += 1;
                                }
                            }
                        }
                        Some('$') => {
                            let (value, next) = expand(&chars, i, &env)?;
                            cur.push_str(&value);
                            i = next;
                        }
                        Some(&other) => {
                            cur.push(other);
                            i += 1;
                        }
                    }
                }
            }
            '\\' => {
                started = true;
                match chars.get(i + 1) {
                    None => return Err(SyntaxError("reached EOF without closing quote \\".into())),
                    Some('\n') => i += 2,
                    Some(&n) => {
                        cur.push(n);
                        i += 2;
                    }
                }
            }
            '$' => {
                let (value, next) = expand(&chars, i, &env)?;
                i = next;
                // An unquoted expansion is split on IFS whitespace.
                let mut parts = value.split([' ', '\t', '\n']).peekable();
                let mut first = true;
                while let Some(part) = parts.next() {
                    if !first {
                        flush!();
                    }
                    first = false;
                    if !part.is_empty() {
                        started = true;
                        cur.push_str(part);
                    } else if parts.peek().is_some() {
                        // An empty leading or interior part just closes the
                        // current field; it never produces one of its own.
                    }
                }
            }
            other => {
                started = true;
                cur.push(other);
                i += 1;
            }
        }
    }
    flush!();
    Ok(out)
}

/// Expands the parameter reference starting at `i` (which points at `$`).
///
/// Returns the expansion and the index just past it.
fn expand<F>(chars: &[char], i: usize, env: &F) -> Result<(String, usize), SyntaxError>
where
    F: Fn(&str) -> Option<String>,
{
    debug_assert_eq!(chars[i], '$');
    match chars.get(i + 1) {
        Some('{') => {
            let mut j = i + 2;
            let mut name = String::new();
            loop {
                match chars.get(j) {
                    None => {
                        return Err(SyntaxError("reached EOF without matching { with }".into()))
                    }
                    Some('}') => {
                        j += 1;
                        break;
                    }
                    Some(&c) => {
                        name.push(c);
                        j += 1;
                    }
                }
            }
            Ok((env(&name).unwrap_or_default(), j))
        }
        Some(c) if c.is_ascii_alphabetic() || *c == '_' => {
            let mut j = i + 1;
            let mut name = String::new();
            while let Some(&c) = chars.get(j) {
                if c.is_ascii_alphanumeric() || c == '_' {
                    name.push(c);
                    j += 1;
                } else {
                    break;
                }
            }
            Ok((env(&name).unwrap_or_default(), j))
        }
        // A lone `$`, or one before a character that cannot start a name, is
        // literal.
        _ => Ok(("$".to_string(), i + 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn splits_on_whitespace() {
        assert_eq!(fields("less -r", no_env).unwrap(), vec!["less", "-r"]);
        assert_eq!(fields("  less   -r  ", no_env).unwrap(), vec!["less", "-r"]);
    }

    #[test]
    fn an_empty_command_has_no_fields() {
        assert!(fields("", no_env).unwrap().is_empty());
        assert!(fields("   ", no_env).unwrap().is_empty());
    }

    #[test]
    fn quotes_keep_spaces_together() {
        assert_eq!(
            fields("'my pager' -x", no_env).unwrap(),
            vec!["my pager", "-x"]
        );
        assert_eq!(
            fields("\"my pager\" -x", no_env).unwrap(),
            vec!["my pager", "-x"]
        );
        assert_eq!(fields("a'' b", no_env).unwrap(), vec!["a", "b"]);
        assert_eq!(fields("''", no_env).unwrap(), vec![""]);
    }

    #[test]
    fn backslash_escapes_a_space() {
        assert_eq!(fields("my\\ pager", no_env).unwrap(), vec!["my pager"]);
    }

    #[test]
    fn expands_variables() {
        let env = |k: &str| match k {
            "P" => Some("less".to_string()),
            "ARGS" => Some("-r -F".to_string()),
            _ => None,
        };
        assert_eq!(fields("$P $ARGS", env).unwrap(), vec!["less", "-r", "-F"]);
        assert_eq!(fields("${P}x", env).unwrap(), vec!["lessx"]);
        assert_eq!(fields("\"$ARGS\"", env).unwrap(), vec!["-r -F"]);
        assert_eq!(fields("$NOPE", env).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn unterminated_quotes_are_an_error() {
        assert!(fields("'oops", no_env).is_err());
        assert!(fields("\"oops", no_env).is_err());
        assert!(fields("${oops", no_env).is_err());
    }
}
