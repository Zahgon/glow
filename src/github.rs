//! GitHub README lookup.

use crate::deps::url::Url;
use crate::http;
use crate::source::Source;

/// Finds the correct README filename in a repository using the GitHub API.
pub fn find_github_readme(u: &Url) -> Result<Source, String> {
    let path = u.path.trim_start_matches('/');
    let (owner, repo) = match path.split_once('/') {
        Some(parts) => parts,
        None => return Err(format!("invalid url: {u}")),
    };

    let api_url = format!(
        "https://api.{}/repos/{}/{}/readme",
        u.hostname(),
        owner,
        repo
    );

    let res = http::get(&api_url).map_err(|e| format!("unable to get url: {e}"))?;
    let body = res
        .into_string()
        .map_err(|e| format!("unable to read http response body: {e}"))?;

    let parsed: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("unable to parse json: {e}"))?;
    let download_url = parsed
        .get("download_url")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    if res.status == 200 {
        let resp = http::get(&download_url).map_err(|e| format!("unable to get url: {e}"))?;
        if resp.status == 200 {
            return Ok(Source {
                reader: resp.into_reader(),
                url: download_url,
            });
        }
    }

    Err("can't find README in GitHub repository".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_is_not_owner_slash_repo_is_rejected_before_any_request() {
        let u = Url::parse("https://github.com/onlyone").expect("a literal URL parses");
        assert_eq!(
            find_github_readme(&u).unwrap_err(),
            "invalid url: https://github.com/onlyone"
        );
    }
}
