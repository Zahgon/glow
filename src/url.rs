//! Resolution of GitHub and GitLab shorthands into README URLs.

use crate::deps::url::Url;
use crate::github::find_github_readme;
use crate::gitlab::find_gitlab_readme;
use crate::source::Source;

const PROTO_GITHUB: &str = "github://";
const PROTO_GITLAB: &str = "gitlab://";
const PROTO_HTTPS: &str = "https://";

/// The canonical GitHub host.
fn github_url() -> Url {
    Url::parse("https://github.com").expect("a literal URL parses")
}

/// The canonical GitLab host.
fn gitlab_url() -> Url {
    Url::parse("https://gitlab.com").expect("a literal URL parses")
}

/// Resolves `path` into the raw README source it names, if it names one.
///
/// Returns `Ok(None)` when the path is not a GitHub or GitLab reference, which
/// is how the caller knows to try the next resolution strategy.
pub fn readme_url(path: &str) -> Result<Option<Source>, String> {
    if path.starts_with(PROTO_GITHUB) {
        return match github_readme_url(path) {
            Some(u) => readme_url(&u.to_string()),
            None => Ok(None),
        };
    }
    if path.starts_with(PROTO_GITLAB) {
        return match gitlab_readme_url(path) {
            Some(u) => readme_url(&u.to_string()),
            None => Ok(None),
        };
    }

    let path = if path.starts_with(PROTO_HTTPS) {
        path.to_string()
    } else {
        format!("{PROTO_HTTPS}{path}")
    };
    let u = Url::parse(&path).map_err(|e| format!("unable to parse url: {e}"))?;

    if u.hostname() == github_url().hostname() {
        return find_github_readme(&u).map(Some);
    }
    if u.hostname() == gitlab_url().hostname() {
        return find_gitlab_readme(&u).map(Some);
    }
    Ok(None)
}

/// Expands `github://owner/repo` into a GitHub URL.
///
/// Returns `None` for anything that is not exactly two path segments, because
/// custom hostnames are not supported.
pub fn github_readme_url(path: &str) -> Option<Url> {
    let path = path.strip_prefix(PROTO_GITHUB).unwrap_or(path);
    if path.split('/').count() != 2 {
        return None;
    }
    Some(github_url().join_path(path))
}

/// Expands `gitlab://owner/repo` into a GitLab URL.
pub fn gitlab_readme_url(path: &str) -> Option<Url> {
    let path = path.strip_prefix(PROTO_GITLAB).unwrap_or(path);
    if path.split('/').count() != 2 {
        return None;
    }
    Some(gitlab_url().join_path(path))
}

/// Whether `path` is a URL: parseable as a request URI and carrying `://`.
pub fn is_url(path: &str) -> bool {
    Url::parse_request_uri(path).is_ok() && path.contains("://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_shorthand_expands_to_a_repository_url() {
        assert_eq!(
            github_readme_url("github://charmbracelet/glow")
                .expect("two segments")
                .to_string(),
            "https://github.com/charmbracelet/glow"
        );
    }

    #[test]
    fn gitlab_shorthand_expands_to_a_repository_url() {
        assert_eq!(
            gitlab_readme_url("gitlab://caarlos0/test")
                .expect("two segments")
                .to_string(),
            "https://gitlab.com/caarlos0/test"
        );
    }

    #[test]
    fn shorthands_with_a_custom_host_are_rejected() {
        assert!(github_readme_url("github://ghe.example.com/owner/repo").is_none());
        assert!(gitlab_readme_url("gitlab://only-one-segment").is_none());
    }

    #[test]
    fn is_url_requires_a_scheme_separator() {
        assert!(is_url("https://example.com/x.md"));
        assert!(!is_url("example.com/x.md"));
        assert!(!is_url("/absolute/path.md"));
        assert!(!is_url("relative.md"));
    }

    #[test]
    fn non_forge_hosts_resolve_to_nothing() {
        assert!(readme_url("example.com/some/path")
            .expect("no error")
            .is_none());
    }
}
