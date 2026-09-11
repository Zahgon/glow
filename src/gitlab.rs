//! GitLab README lookup.

use crate::deps::url::{query_escape, Url};
use crate::http;
use crate::source::Source;

/// Finds the correct README filename in a repository using the GitLab API.
pub fn find_gitlab_readme(u: &Url) -> Result<Source, String> {
    let path = u.path.trim_start_matches('/');
    let (owner, repo) = match path.split_once('/') {
        Some(parts) => parts,
        None => return Err(format!("invalid url: {u}")),
    };

    let project_path = query_escape(&format!("{owner}/{repo}"));
    let api_url = format!("https://{}/api/v4/projects/{}", u.hostname(), project_path);

    let res = http::get(&api_url).map_err(|e| format!("unable to get url: {e}"))?;
    let body = res
        .into_string()
        .map_err(|e| format!("unable to read http response body: {e}"))?;

    let parsed: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("unable to parse json: {e}"))?;
    let readme_url = parsed
        .get("readme_url")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let readme_raw_url = readme_url.replace("blob", "raw");

    if res.status == 200 {
        let resp = http::get(&readme_raw_url).map_err(|e| format!("unable to get url: {e}"))?;
        if resp.status == 200 {
            return Ok(Source {
                reader: resp.into_reader(),
                url: readme_raw_url,
            });
        }
    }

    Err("can't find README in GitLab repository".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_is_not_owner_slash_repo_is_rejected_before_any_request() {
        let u = Url::parse("https://gitlab.com/onlyone").expect("a literal URL parses");
        assert_eq!(
            find_gitlab_readme(&u).unwrap_err(),
            "invalid url: https://gitlab.com/onlyone"
        );
    }
}
