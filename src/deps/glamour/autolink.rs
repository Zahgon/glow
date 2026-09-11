//! Shortening of GitHub URLs in table footnote lists.
//!
//! Reimplements `glamour/internal/autolink`, whose patterns turn a long GitHub
//! URL into `owner/repo#123`, `owner/repo@abc1234` and friends.

/// Returns the shortened form of a GitHub URL, if one of the patterns matches.
pub fn detect(u: &str) -> Option<String> {
    let rest = u
        .strip_prefix("https://github.com/")
        .or_else(|| u.strip_prefix("http://github.com/"))?;

    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() < 4 {
        return None;
    }
    let owner = parts[0];
    let repo = parts[1];
    if !is_name(owner) || !is_name(repo) {
        return None;
    }

    match parts[2] {
        "issue" | "issues" | "pull" | "pulls" | "discussion" | "discussions" => {
            let (number, anchor) = split_anchor(parts[3]);
            if !number.chars().all(|c| c.is_ascii_digit()) || number.is_empty() {
                return None;
            }
            // `.../pull/12/commits/<sha>` shortens to a commit reference.
            if parts.len() >= 6
                && (parts[2] == "pull" || parts[2] == "pulls")
                && parts[4] == "commits"
            {
                let (sha, _) = split_anchor(parts[5]);
                if sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_alphanumeric()) {
                    return Some(format!("{owner}/{repo}@{}", &sha[..7]));
                }
                return None;
            }
            if parts.len() != 4 {
                return None;
            }
            match anchor {
                None => Some(format!("{owner}/{repo}#{number}")),
                Some(a)
                    if a.starts_with("issuecomment-") || a.starts_with("discussioncomment-") =>
                {
                    Some(format!("{owner}/{repo}#{number} (comment)"))
                }
                Some(a) if a.starts_with("discussion_r") => {
                    Some(format!("{owner}/{repo}#{number} (comment)"))
                }
                Some(a) if a.starts_with("pullrequestreview-") => {
                    Some(format!("{owner}/{repo}#{number} (review)"))
                }
                Some(_) => None,
            }
        }
        "commit" => {
            let (sha, _) = split_anchor(parts[3]);
            if sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_alphanumeric()) {
                Some(format!("{owner}/{repo}@{}", &sha[..7]))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn split_anchor(s: &str) -> (&str, Option<&str>) {
    match s.find('#') {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    }
}

fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_issues_and_pulls() {
        assert_eq!(
            detect("https://github.com/charmbracelet/glow/issues/42").as_deref(),
            Some("charmbracelet/glow#42")
        );
        assert_eq!(
            detect("https://github.com/charmbracelet/glow/pull/7").as_deref(),
            Some("charmbracelet/glow#7")
        );
    }

    #[test]
    fn shortens_comments_and_reviews() {
        assert_eq!(
            detect("https://github.com/a/b/issues/1#issuecomment-999").as_deref(),
            Some("a/b#1 (comment)")
        );
        assert_eq!(
            detect("https://github.com/a/b/pull/1#pullrequestreview-5").as_deref(),
            Some("a/b#1 (review)")
        );
    }

    #[test]
    fn shortens_commits() {
        assert_eq!(
            detect("https://github.com/a/b/commit/0123456789abcdef").as_deref(),
            Some("a/b@0123456")
        );
    }

    #[test]
    fn leaves_other_urls_alone() {
        assert!(detect("https://example.com/a/b").is_none());
        assert!(detect("https://github.com/a/b").is_none());
    }
}
