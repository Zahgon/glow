//! Ordering for the file listing.

use super::markdown::Markdown;

/// Sorts documents by note, keeping the order of equal notes.
pub fn sort_markdowns(mds: &mut [Markdown]) {
    mds.sort_by(|a, b| a.note.cmp(&b.note));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md(note: &str, path: &str) -> Markdown {
        Markdown {
            note: note.into(),
            local_path: path.into(),
            ..Markdown::default()
        }
    }

    #[test]
    fn documents_sort_by_note() {
        let mut mds = vec![md("c.md", "1"), md("a.md", "2"), md("b.md", "3")];
        sort_markdowns(&mut mds);
        let notes: Vec<&str> = mds.iter().map(|m| m.note.as_str()).collect();
        assert_eq!(notes, vec!["a.md", "b.md", "c.md"]);
    }

    #[test]
    fn the_sort_is_stable() {
        let mut mds = vec![md("a.md", "1"), md("a.md", "2"), md("a.md", "3")];
        sort_markdowns(&mut mds);
        let paths: Vec<&str> = mds.iter().map(|m| m.local_path.as_str()).collect();
        assert_eq!(paths, vec!["1", "2", "3"]);
    }
}
