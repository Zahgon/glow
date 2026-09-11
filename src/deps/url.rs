//! URL parsing with Go `net/url` semantics.
//!
//! Glow's source resolution, base-URL derivation and hyperlink emission all
//! depend on the exact behaviour of `url.Parse`, `url.ParseRequestURI`,
//! `URL.Hostname`, `URL.JoinPath`, `URL.ResolveReference` and `URL.String`,
//! including their permissiveness — `url.Parse` accepts almost any string as a
//! relative reference, which is why `isURL` also tests for `"://"`.

use std::fmt;

/// A parse failure, carrying Go's message shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParseError {}

/// A parsed URL.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Url {
    /// Scheme, without the `:`.
    pub scheme: String,
    /// Authority, host and port.
    pub host: String,
    /// Path, percent-decoded is not attempted; kept as written.
    pub path: String,
    /// Query string without the `?`.
    pub raw_query: String,
    /// Fragment without the `#`.
    pub fragment: String,
    /// Whether the URL had a `//` authority component.
    pub has_authority: bool,
}

impl Url {
    /// Parses a URL, accepting relative references as Go's `url.Parse` does.
    pub fn parse(raw: &str) -> Result<Url, ParseError> {
        if raw.chars().any(|c| c.is_control()) {
            return Err(ParseError(format!(
                "parse {raw:?}: net/url: invalid control character in URL"
            )));
        }
        let mut u = Url::default();
        let mut rest = raw;

        if let Some(i) = rest.find('#') {
            u.fragment = rest[i + 1..].to_string();
            rest = &rest[..i];
        }
        if let Some(i) = rest.find('?') {
            u.raw_query = rest[i + 1..].to_string();
            rest = &rest[..i];
        }

        // A scheme is `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ":"`.
        if let Some(i) = rest.find(':') {
            let candidate = &rest[..i];
            if is_scheme(candidate) {
                u.scheme = candidate.to_ascii_lowercase();
                rest = &rest[i + 1..];
            }
        }

        if let Some(after) = rest.strip_prefix("//") {
            u.has_authority = true;
            let end = after.find('/').unwrap_or(after.len());
            u.host = after[..end].to_string();
            if let Some(port) = port_of(&u.host) {
                if !port.is_empty() && !port.chars().all(|c| c.is_ascii_digit()) {
                    return Err(ParseError(format!(
                        "parse {raw:?}: invalid port \":{port}\" after host"
                    )));
                }
            }
            rest = &after[end..];
        }
        u.path = rest.to_string();
        Ok(u)
    }

    /// Parses an absolute URL or an absolute path, as `url.ParseRequestURI` does.
    pub fn parse_request_uri(raw: &str) -> Result<Url, ParseError> {
        let u = Url::parse(raw)?;
        if u.scheme.is_empty() && !u.path.starts_with('/') {
            return Err(ParseError(format!(
                "parse {raw:?}: invalid URI for request"
            )));
        }
        if !u.fragment.is_empty() || raw.contains('#') {
            return Err(ParseError(format!(
                "parse {raw:?}: invalid URI for request"
            )));
        }
        Ok(u)
    }

    /// The host without its port, and without IPv6 brackets.
    pub fn hostname(&self) -> String {
        let host = &self.host;
        if let Some(stripped) = host.strip_prefix('[') {
            if let Some(i) = stripped.find(']') {
                return stripped[..i].to_string();
            }
        }
        match host.rfind(':') {
            Some(i) if !host[i + 1..].contains(']') => host[..i].to_string(),
            _ => host.clone(),
        }
    }

    /// The fragment, without its `#`.
    pub fn fragment(&self) -> &str {
        &self.fragment
    }

    /// True when the URL carries a scheme.
    pub fn is_abs(&self) -> bool {
        !self.scheme.is_empty()
    }

    /// Appends path elements, as `URL.JoinPath` does.
    pub fn join_path(&self, elems: &str) -> Url {
        let mut u = self.clone();
        let mut path = u.path.clone();
        if !path.ends_with('/') && !elems.is_empty() {
            path.push('/');
        }
        path.push_str(elems.trim_start_matches('/'));
        u.path = path;
        u
    }

    /// Resolves `reference` against this URL, as `URL.ResolveReference` does.
    pub fn resolve_reference(&self, reference: &Url) -> Url {
        if reference.is_abs() {
            return reference.clone();
        }
        let mut out = self.clone();
        out.raw_query = reference.raw_query.clone();
        out.fragment = reference.fragment.clone();
        if reference.has_authority {
            out.host = reference.host.clone();
            out.has_authority = true;
            out.path = resolve_path(&reference.path, "");
            return out;
        }
        if reference.path.is_empty() {
            if reference.raw_query.is_empty() {
                out.raw_query = self.raw_query.clone();
            }
            return out;
        }
        out.path = resolve_path(&self.path, &reference.path);
        out
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.scheme.is_empty() {
            write!(f, "{}:", self.scheme)?;
        }
        if self.has_authority || !self.host.is_empty() {
            write!(f, "//{}", self.host)?;
        }
        write!(f, "{}", self.path)?;
        if !self.raw_query.is_empty() {
            write!(f, "?{}", self.raw_query)?;
        }
        if !self.fragment.is_empty() {
            write!(f, "#{}", self.fragment)?;
        }
        Ok(())
    }
}

fn is_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

fn port_of(host: &str) -> Option<&str> {
    if host.starts_with('[') {
        let end = host.find(']')?;
        return host[end + 1..].strip_prefix(':');
    }
    let i = host.rfind(':')?;
    Some(&host[i + 1..])
}

/// Merges a reference path onto a base path and removes `.`/`..` segments.
fn resolve_path(base: &str, reference: &str) -> String {
    let full = if reference.is_empty() {
        base.to_string()
    } else if reference.starts_with('/') {
        reference.to_string()
    } else {
        match base.rfind('/') {
            Some(i) => format!("{}{}", &base[..i + 1], reference),
            None => reference.to_string(),
        }
    };

    let mut out: Vec<&str> = Vec::new();
    let trailing_slash = full.ends_with('/') || full.ends_with("/.") || full.ends_with("/..");
    for seg in full.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    let mut path = String::from("/");
    path.push_str(&out.join("/"));
    if trailing_slash && !path.ends_with('/') {
        path.push('/');
    }
    path
}

/// Resolves `rel` against `base`, exactly as Glamour's `resolveRelativeURL` does.
pub fn resolve_reference(base: &str, rel: &str) -> String {
    let mut u = match Url::parse(rel) {
        Ok(u) => u,
        Err(_) => return rel.to_string(),
    };
    if u.is_abs() {
        return rel.to_string();
    }
    u.path = u.path.trim_start_matches('/').to_string();
    let base_url = match Url::parse(base) {
        Ok(b) => b,
        Err(_) => return rel.to_string(),
    };
    base_url.resolve_reference(&u).to_string()
}

/// Percent-encodes a string for use in a query, as `url.QueryEscape` does.
pub fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `filepath.Dir` semantics, used to derive a document's base URL.
pub fn dir(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => path[..i].to_string(),
        None => ".".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_absolute_urls() {
        let u = Url::parse("https://github.com/charmbracelet/glow").expect("parses");
        assert_eq!(u.scheme, "https");
        assert_eq!(u.host, "github.com");
        assert_eq!(u.path, "/charmbracelet/glow");
        assert_eq!(u.hostname(), "github.com");
    }

    #[test]
    fn strips_port_from_hostname() {
        let u = Url::parse("https://example.com:8443/x").expect("parses");
        assert_eq!(u.hostname(), "example.com");
    }

    #[test]
    fn parse_accepts_relative_references() {
        let u = Url::parse("plain text").expect("parses");
        assert_eq!(u.path, "plain text");
        assert!(!u.is_abs());
    }

    #[test]
    fn parse_request_uri_rejects_relative() {
        assert!(Url::parse_request_uri("nope.md").is_err());
        assert!(Url::parse_request_uri("/abs/path.md").is_ok());
        assert!(Url::parse_request_uri("https://x.example/a").is_ok());
    }

    #[test]
    fn join_path_appends_segments() {
        let base = Url::parse("https://github.com").expect("parses");
        assert_eq!(
            base.join_path("charmbracelet/glow").to_string(),
            "https://github.com/charmbracelet/glow"
        );
    }

    #[test]
    fn resolves_relative_references() {
        assert_eq!(
            resolve_reference("https://example.com/a/", "b.png"),
            "https://example.com/a/b.png"
        );
        assert_eq!(
            resolve_reference("https://example.com/a/", "https://other.example/x"),
            "https://other.example/x"
        );
    }

    #[test]
    fn query_escape_matches_go() {
        assert_eq!(query_escape("caarlos0/test"), "caarlos0%2Ftest");
    }

    #[test]
    fn dir_matches_filepath_dir() {
        assert_eq!(dir("/a/b/c.md"), "/a/b");
        assert_eq!(dir("/c.md"), "/");
        assert_eq!(dir("c.md"), ".");
    }

    #[test]
    fn fragment_only_url_round_trips() {
        let u = Url::parse("#section").expect("parses");
        assert_eq!(u.fragment, "section");
        assert_eq!(u.path, "");
    }
}
