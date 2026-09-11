//! The original `url_test.go`, carried over.
//!
//! `TestURLParser` is a table over eight inputs, each run as a subtest that
//! skips with "test uses network, sometimes fails for no reason". The table and
//! the checks that do not need the network stay in [`test_url_parser`]; the
//! eight network assertions keep one test each, named after their input, and
//! are ignored by default. Run them with
//! `cargo test --test url_test -- --ignored`.

use glow::deps::url::Url;
use glow::url::{github_readme_url, gitlab_readme_url, is_url, readme_url};

/// Input and the README URL it resolves to.
const CASES: [(&str, &str); 8] = [
    (
        "github.com/charmbracelet/glow",
        "https://raw.githubusercontent.com/charmbracelet/glow/master/README.md",
    ),
    (
        "github://charmbracelet/glow",
        "https://raw.githubusercontent.com/charmbracelet/glow/master/README.md",
    ),
    (
        "github://caarlos0/dotfiles.fish",
        "https://raw.githubusercontent.com/caarlos0/dotfiles.fish/main/README.md",
    ),
    (
        "github://tj/git-extras",
        "https://raw.githubusercontent.com/tj/git-extras/main/Readme.md",
    ),
    (
        "https://github.com/goreleaser/nfpm",
        "https://raw.githubusercontent.com/goreleaser/nfpm/main/README.md",
    ),
    (
        "gitlab.com/caarlos0/test",
        "https://gitlab.com/caarlos0/test/-/raw/master/README.md",
    ),
    (
        "gitlab://caarlos0/test",
        "https://gitlab.com/caarlos0/test/-/raw/master/README.md",
    ),
    (
        "https://gitlab.com/terrakok/gitlab-client",
        "https://gitlab.com/terrakok/gitlab-client/-/raw/develop/Readme.md",
    ),
];

/// Resolves one case against the live forge API.
fn assert_readme_url(input: &str, expected: &str) {
    let src = readme_url(input)
        .unwrap_or_else(|e| panic!("{input}: {e}"))
        .unwrap_or_else(|| panic!("{input}: no source"));
    assert_eq!(src.url, expected, "{input}");
}

/// The half of each case that can be checked without reaching the network:
/// every input names a repository on a forge glow knows, and every expected URL
/// is that same repository's raw README on that same forge.
#[test]
fn test_url_parser() {
    for (input, expected) in CASES {
        // The shorthands expand without a network round trip; the rest already
        // carry a host.
        let repo = if let Some(u) = github_readme_url(input) {
            assert!(input.starts_with("github://"), "{input}");
            u
        } else if let Some(u) = gitlab_readme_url(input) {
            assert!(input.starts_with("gitlab://"), "{input}");
            u
        } else {
            let with_scheme = if is_url(input) {
                input.to_string()
            } else {
                format!("https://{input}")
            };
            Url::parse(&with_scheme).unwrap_or_else(|e| panic!("{input}: {e}"))
        };

        let (owner, name) = repo
            .path
            .trim_start_matches('/')
            .split_once('/')
            .unwrap_or_else(|| panic!("{input}: not owner/repo"));

        match repo.hostname().as_str() {
            "github.com" => assert!(
                expected.starts_with(&format!(
                    "https://raw.githubusercontent.com/{owner}/{name}/"
                )),
                "{input} -> {expected}"
            ),
            "gitlab.com" => assert!(
                expected.starts_with(&format!("https://gitlab.com/{owner}/{name}/-/raw/")),
                "{input} -> {expected}"
            ),
            other => panic!("{input}: unsupported host {other}"),
        }

        assert!(
            expected.ends_with("/README.md") || expected.ends_with("/Readme.md"),
            "{input} -> {expected}"
        );
    }
}

#[test]
#[ignore = "test uses network, sometimes fails for no reason"]
fn github_com_charmbracelet_glow() {
    let (input, expected) = CASES[0];
    assert_readme_url(input, expected);
}

#[test]
#[ignore = "test uses network, sometimes fails for no reason"]
fn github_charmbracelet_glow() {
    let (input, expected) = CASES[1];
    assert_readme_url(input, expected);
}

#[test]
#[ignore = "test uses network, sometimes fails for no reason"]
fn github_caarlos0_dotfiles_fish() {
    let (input, expected) = CASES[2];
    assert_readme_url(input, expected);
}

#[test]
#[ignore = "test uses network, sometimes fails for no reason"]
fn github_tj_git_extras() {
    let (input, expected) = CASES[3];
    assert_readme_url(input, expected);
}

#[test]
#[ignore = "test uses network, sometimes fails for no reason"]
fn https_github_com_goreleaser_nfpm() {
    let (input, expected) = CASES[4];
    assert_readme_url(input, expected);
}

#[test]
#[ignore = "test uses network, sometimes fails for no reason"]
fn gitlab_com_caarlos0_test() {
    let (input, expected) = CASES[5];
    assert_readme_url(input, expected);
}

#[test]
#[ignore = "test uses network, sometimes fails for no reason"]
fn gitlab_caarlos0_test() {
    let (input, expected) = CASES[6];
    assert_readme_url(input, expected);
}

#[test]
#[ignore = "test uses network, sometimes fails for no reason"]
fn https_gitlab_com_terrakok_gitlab_client() {
    let (input, expected) = CASES[7];
    assert_readme_url(input, expected);
}
