//! Searching a directory tree for files.
//!
//! Reimplements `github.com/muesli/gitcha`: a lexical walk that streams matches
//! as it finds them, honouring the `.gitignore` of the enclosing repository and
//! a list of exclusion patterns.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver};
use std::time::SystemTime;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// The absolute path of a file, with the metadata the walk already had.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Absolute path of the file.
    pub path: String,
    /// Last modification time.
    pub modified: SystemTime,
}

/// Returns the directory of the git repository `path` belongs to, if any.
pub fn git_repo_for_path(path: &str) -> Option<String> {
    let mut dir = PathBuf::from(path);
    if !dir.is_absolute() {
        dir = std::env::current_dir().ok()?.join(dir);
    }
    loop {
        if dir.join(".git").is_dir() {
            return Some(dir.to_string_lossy().into_owned());
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => return None,
        }
    }
}

/// Streams every file under `path` whose name matches one of `list`.
///
/// `ignore_patterns` are `filepath.Match` globs; a pattern without a separator
/// is matched against the directory currently being walked.
/// `respect_git_ignore` additionally applies the repository's `.gitignore`.
pub fn find_files(
    path: &str,
    list: &[String],
    ignore_patterns: &[String],
    respect_git_ignore: bool,
) -> Result<Receiver<SearchResult>, String> {
    let abs = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !abs.is_dir() {
        return Err(format!("{}: not a directory", abs.display()));
    }

    let (tx, rx) = sync_channel::<SearchResult>(0);
    let list: Vec<String> = list.to_vec();
    let patterns: Vec<String> = ignore_patterns.to_vec();

    std::thread::spawn(move || {
        let mut last_git = String::new();
        let mut gi: Option<Gitignore> = None;
        let mut stack = vec![abs.clone()];

        while let Some(current) = stack.pop() {
            let meta = match std::fs::symlink_metadata(&current) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let is_dir = current.is_dir();
            let path_str = current.to_string_lossy().into_owned();

            if respect_git_ignore {
                if let Some(git) = git_repo_for_path(&path_str) {
                    if git != path_str {
                        if last_git != git {
                            last_git = git.clone();
                            let mut b = GitignoreBuilder::new(&git);
                            b.add(Path::new(&git).join(".gitignore"));
                            gi = b.build().ok();
                        }
                        if let Some(g) = &gi {
                            if g.matched_path_or_any_parents(&current, is_dir).is_ignore() {
                                continue;
                            }
                        }
                    }
                }
            }

            let mut skipped = false;
            for pattern in &patterns {
                let pattern = if !pattern.contains('/') {
                    match current.parent() {
                        Some(dir) if dir.as_os_str() != "." && !dir.as_os_str().is_empty() => {
                            format!("{}/{pattern}", dir.to_string_lossy())
                        }
                        _ => continue,
                    }
                } else {
                    pattern.clone()
                };
                if glob_match(&pattern, &path_str) {
                    skipped = true;
                    break;
                }
            }
            if skipped {
                continue;
            }

            if is_dir {
                if let Ok(entries) = std::fs::read_dir(&current) {
                    let mut paths: Vec<PathBuf> =
                        entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
                    // `filepath.Walk` visits lexically; the stack reverses it.
                    paths.sort();
                    paths.reverse();
                    stack.extend(paths);
                }
                continue;
            }

            let base = current
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            for v in &list {
                let matched = base.eq_ignore_ascii_case(v)
                    || glob_match(&v.to_lowercase(), &base.to_lowercase());
                if matched {
                    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    if tx
                        .send(SearchResult {
                            path: path_str.clone(),
                            modified,
                        })
                        .is_err()
                    {
                        return;
                    }
                    break;
                }
            }
        }
    });

    Ok(rx)
}

/// `filepath.Match`: shell-style globbing with no `**` and no `/` crossing.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    match_from(&p, 0, &n, 0)
}

fn match_from(p: &[char], mut pi: usize, n: &[char], mut ni: usize) -> bool {
    while pi < p.len() {
        match p[pi] {
            '*' => {
                // `*` never matches a separator.
                let mut k = ni;
                loop {
                    if match_from(p, pi + 1, n, k) {
                        return true;
                    }
                    if k >= n.len() || n[k] == '/' {
                        return false;
                    }
                    k += 1;
                }
            }
            '?' => {
                if ni >= n.len() || n[ni] == '/' {
                    return false;
                }
                ni += 1;
                pi += 1;
            }
            '[' => {
                if ni >= n.len() {
                    return false;
                }
                let (ok, next) = match_class(p, pi, n[ni]);
                if !ok {
                    return false;
                }
                pi = next;
                ni += 1;
            }
            '\\' if pi + 1 < p.len() => {
                if ni >= n.len() || n[ni] != p[pi + 1] {
                    return false;
                }
                pi += 2;
                ni += 1;
            }
            c => {
                if ni >= n.len() || n[ni] != c {
                    return false;
                }
                pi += 1;
                ni += 1;
            }
        }
    }
    ni == n.len()
}

/// Matches one `[...]` character class, returning whether it matched and the
/// index just past the closing bracket.
fn match_class(p: &[char], start: usize, c: char) -> (bool, usize) {
    let mut i = start + 1;
    let negated = p.get(i) == Some(&'^');
    if negated {
        i += 1;
    }
    let mut matched = false;
    let mut first = true;
    while i < p.len() && (p[i] != ']' || first) {
        first = false;
        let lo = p[i];
        i += 1;
        if p.get(i) == Some(&'-') && p.get(i + 1).is_some_and(|&x| x != ']') {
            let hi = p[i + 1];
            i += 2;
            if lo <= c && c <= hi {
                matched = true;
            }
        } else if lo == c {
            matched = true;
        }
    }
    if i < p.len() {
        i += 1; // step past `]`
    }
    (matched != negated, i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_do_not_cross_separators() {
        assert!(glob_match("*.md", "readme.md"));
        assert!(!glob_match("*.md", "docs/readme.md"));
        assert!(glob_match("/a/*/c", "/a/b/c"));
        assert!(!glob_match("/a/*/c", "/a/b/x/c"));
        assert!(glob_match(".*", ".git"));
        assert!(!glob_match(".*", "git"));
    }

    #[test]
    fn character_classes_and_escapes_work() {
        assert!(glob_match("[abc].md", "b.md"));
        assert!(!glob_match("[abc].md", "d.md"));
        assert!(glob_match("[a-c].md", "c.md"));
        assert!(glob_match("[^a].md", "b.md"));
        assert!(glob_match("a\\*b", "a*b"));
    }

    #[test]
    fn finds_markdown_files_in_lexical_order() {
        let dir = tempfile::tempdir().expect("a temp dir");
        std::fs::write(dir.path().join("b.md"), "b").expect("write");
        std::fs::write(dir.path().join("a.md"), "a").expect("write");
        std::fs::write(dir.path().join("c.txt"), "c").expect("write");
        std::fs::create_dir(dir.path().join("sub")).expect("mkdir");
        std::fs::write(dir.path().join("sub/d.md"), "d").expect("write");

        let rx = find_files(
            &dir.path().to_string_lossy(),
            &["*.md".to_string()],
            &[],
            false,
        )
        .expect("a search");
        let mut names: Vec<String> = rx
            .iter()
            .map(|r| {
                Path::new(&r.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, vec!["a.md", "b.md", "d.md"]);
        names.clear();
    }

    #[test]
    fn ignore_patterns_skip_whole_directories() {
        let dir = tempfile::tempdir().expect("a temp dir");
        std::fs::create_dir(dir.path().join("node_modules")).expect("mkdir");
        std::fs::write(dir.path().join("node_modules/x.md"), "x").expect("write");
        std::fs::write(dir.path().join("keep.md"), "k").expect("write");

        let rx = find_files(
            &dir.path().to_string_lossy(),
            &["*.md".to_string()],
            &["node_modules".to_string()],
            false,
        )
        .expect("a search");
        let names: Vec<String> = rx
            .iter()
            .map(|r| {
                Path::new(&r.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, vec!["keep.md"]);
    }

    #[test]
    fn a_gitignore_hides_matching_files() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("canonical");
        std::fs::create_dir(root.join(".git")).expect("mkdir");
        std::fs::write(root.join(".gitignore"), "hidden.md\n").expect("write");
        std::fs::write(root.join("hidden.md"), "h").expect("write");
        std::fs::write(root.join("shown.md"), "s").expect("write");

        let rx = find_files(&root.to_string_lossy(), &["*.md".to_string()], &[], true)
            .expect("a search");
        let names: Vec<String> = rx
            .iter()
            .map(|r| {
                Path::new(&r.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, vec!["shown.md"]);
    }
}
