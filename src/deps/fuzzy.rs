//! Fuzzy string matching.
//!
//! Reimplements `github.com/sahilm/fuzzy`: a Sublime-Text-style matcher that
//! scores a sequential, case-insensitive character match with bonuses for the
//! first character, camel-case boundaries, separators and adjacency, and
//! penalties for unmatched leading and trailing characters.

/// A matched string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// The matched string.
    pub str: String,
    /// The index of the matched string in the supplied slice.
    pub index: usize,
    /// Byte offsets of the matched characters, for highlighting.
    pub matched_indexes: Vec<usize>,
    /// Rank of the match; higher is better.
    pub score: i64,
}

const FIRST_CHAR_MATCH_BONUS: i64 = 10;
const MATCH_FOLLOWING_SEPARATOR_BONUS: i64 = 20;
const CAMEL_CASE_MATCH_BONUS: i64 = 20;
const ADJACENT_MATCH_BONUS: i64 = 5;
const UNMATCHED_LEADING_CHAR_PENALTY: i64 = -5;
const MAX_UNMATCHED_LEADING_CHAR_PENALTY: i64 = -15;

const SEPARATORS: [char; 6] = ['/', '-', '_', ' ', '.', '\\'];

/// Looks `pattern` up in `data`, best match first.
///
/// The sort is stable, so equally-scored entries keep their input order.
pub fn find(pattern: &str, data: &[String]) -> Vec<Match> {
    let mut matches = find_no_sort(pattern, data);
    matches.sort_by_key(|m| std::cmp::Reverse(m.score));
    matches
}

/// Looks `pattern` up in `data`, in input order.
pub fn find_no_sort(pattern: &str, data: &[String]) -> Vec<Match> {
    if pattern.is_empty() {
        return Vec::new();
    }
    let runes: Vec<char> = pattern.chars().collect();
    let mut matches = Vec::new();

    for (index, match_str) in data.iter().enumerate() {
        // Matching stops at the first NUL, which the Go version treats as a
        // caller error rather than data.
        let clean = match match_str.find('\0') {
            Some(i) => &match_str[..i],
            None => match_str.as_str(),
        };

        let mut m = Match {
            str: match_str.clone(),
            index,
            matched_indexes: Vec::with_capacity(runes.len()),
            score: 0,
        };

        let chars: Vec<(usize, char)> = clean.char_indices().collect();
        let mut pattern_index = 0usize;
        let mut score;
        let mut best_score: i64 = -1;
        let mut matched_index: i64 = -1;
        let mut curr_adjacent_match_bonus: i64 = 0;
        let mut last: Option<char> = None;
        let mut last_index: i64 = 0;

        for (n, &(j, candidate)) in chars.iter().enumerate() {
            if pattern_index < runes.len() && equal_fold(candidate, runes[pattern_index]) {
                score = 0;
                if j == 0 {
                    score += FIRST_CHAR_MATCH_BONUS;
                }
                if last.is_some_and(|l| l.is_lowercase()) && candidate.is_uppercase() {
                    score += CAMEL_CASE_MATCH_BONUS;
                }
                if j != 0 && last.is_some_and(is_separator) {
                    score += MATCH_FOLLOWING_SEPARATOR_BONUS;
                }
                if let Some(&last_match) = m.matched_indexes.last() {
                    let bonus = adjacent_char_bonus(
                        last_index,
                        last_match as i64,
                        curr_adjacent_match_bonus,
                    );
                    score += bonus;
                    // Adjacent matches compound, so the running bonus is kept.
                    curr_adjacent_match_bonus += bonus;
                }
                if score > best_score {
                    best_score = score;
                    matched_index = j as i64;
                }
            }

            let nextp = if pattern_index + 1 < runes.len() {
                runes[pattern_index + 1]
            } else {
                '\0'
            };
            let nextc = chars.get(n + 1).map(|&(_, c)| c).unwrap_or('\0');

            // The best score is banked once the next pattern character is up,
            // or the string has ended — which is what lets "tk" match the
            // second "k" of "The Black Knight".
            if (equal_fold(nextp, nextc) || nextc == '\0') && matched_index > -1 {
                if m.matched_indexes.is_empty() {
                    let penalty = matched_index * UNMATCHED_LEADING_CHAR_PENALTY;
                    best_score += penalty.max(MAX_UNMATCHED_LEADING_CHAR_PENALTY);
                }
                m.score += best_score;
                m.matched_indexes.push(matched_index as usize);
                best_score = -1;
                matched_index = -1;
                pattern_index += 1;
            }

            last_index = j as i64;
            last = Some(candidate);
        }

        // A penalty for every character that was not matched.
        m.score += m.matched_indexes.len() as i64 - clean.len() as i64;
        if m.matched_indexes.len() == runes.len() {
            matches.push(m);
        }
    }
    matches
}

/// `strings.EqualFold` for a single pair of characters.
fn equal_fold(a: char, b: char) -> bool {
    if a == b {
        return true;
    }
    a.to_lowercase().eq(b.to_lowercase())
}

fn adjacent_char_bonus(i: i64, last_match: i64, current_bonus: i64) -> i64 {
    if last_match == i {
        current_bonus * 2 + ADJACENT_MATCH_BONUS
    } else {
        0
    }
}

fn is_separator(c: char) -> bool {
    SEPARATORS.contains(&c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// Scores and match indexes recorded from `sahilm/fuzzy` itself.
    fn assert_matches(pattern: &str, items: &[&str], want: &[(&str, i64, &[usize])]) {
        let got = find(pattern, &data(items));
        let got: Vec<(&str, i64, Vec<usize>)> = got
            .iter()
            .map(|m| (m.str.as_str(), m.score, m.matched_indexes.clone()))
            .collect();
        let want: Vec<(&str, i64, Vec<usize>)> = want
            .iter()
            .map(|(s, sc, idx)| (*s, *sc, idx.to_vec()))
            .collect();
        assert_eq!(got, want, "pattern {pattern:?}");
    }

    #[test]
    fn an_empty_pattern_matches_nothing() {
        assert!(find("", &data(&["anything"])).is_empty());
    }

    #[test]
    fn only_sequential_matches_survive() {
        assert_matches(
            "abc",
            &["a_b_c", "cba", "xabcx", "ab"],
            &[("a_b_c", 48, &[0, 2, 4]), ("xabcx", 13, &[1, 2, 3])],
        );
    }

    #[test]
    fn matching_is_case_insensitive_and_prefers_separators() {
        assert_matches("RM", &["README.md"], &[("README.md", 23, &[0, 7])]);
    }

    #[test]
    fn separators_and_first_characters_earn_a_bonus() {
        assert_matches(
            "ab",
            &["a-b", "xaxb"],
            &[("a-b", 29, &[0, 2]), ("xaxb", -7, &[1, 3])],
        );
    }

    #[test]
    fn exhaustive_matching_picks_the_better_later_character() {
        assert_matches(
            "tk",
            &["The Black Knight"],
            &[("The Black Knight", 16, &[0, 10])],
        );
    }

    #[test]
    fn adjacency_compounds() {
        assert_matches("abc", &["abc"], &[("abc", 30, &[0, 1, 2])]);
    }

    #[test]
    fn the_sort_is_stable_for_equal_scores() {
        assert_matches(
            "a",
            &["ba", "ca", "da"],
            &[("ba", -6, &[1]), ("ca", -6, &[1]), ("da", -6, &[1])],
        );
    }

    #[test]
    fn leading_characters_are_penalised_up_to_a_limit() {
        assert_matches(
            "x",
            &["ax", "aaaaaaaaaax"],
            &[("ax", -6, &[1]), ("aaaaaaaaaax", -25, &[10])],
        );
    }
}
