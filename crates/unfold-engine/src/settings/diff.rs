//! The change an edit makes to `unfold.toml`, as a unified diff of lines.

use std::fmt::Write as _;

use super::SETTINGS_PATH;

/// Lines of context around each change.
const CONTEXT: usize = 3;

/// One line of the comparison.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Line<'a> {
    Same(&'a str),
    Removed(&'a str),
    Added(&'a str),
}

/// The unified diff from `before` to `after`, with `unfold.toml` as both file names; empty
/// when they are equal.
pub(super) fn unified(before: &str, after: &str) -> String {
    if before == after {
        return String::new();
    }
    let old: Vec<&str> = before.split_inclusive('\n').collect();
    let new: Vec<&str> = after.split_inclusive('\n').collect();
    let lines = compare(&old, &new);
    let mut out = format!("--- a/{SETTINGS_PATH}\n+++ b/{SETTINGS_PATH}\n");
    for hunk in hunks(&lines) {
        write_hunk(&mut out, &lines, hunk);
    }
    out
}

/// Every line of both texts in order: the common prefix and suffix as they are, and the middle
/// by a longest common subsequence (edits change a few nearby lines, so the middle is small).
fn compare<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Line<'a>> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (a, b) = (&old[prefix..old.len() - suffix], &new[prefix..new.len() - suffix]);
    // lengths[i][j]: the longest common subsequence of a[i..] and b[j..].
    let mut lengths = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lengths[i][j] = if a[i] == b[j] {
                lengths[i + 1][j + 1] + 1
            } else {
                lengths[i + 1][j].max(lengths[i][j + 1])
            };
        }
    }
    let mut lines: Vec<Line<'a>> = old[..prefix].iter().map(|line| Line::Same(line)).collect();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            lines.push(Line::Same(a[i]));
            i += 1;
            j += 1;
        } else if i < a.len() && (j == b.len() || lengths[i + 1][j] >= lengths[i][j + 1]) {
            lines.push(Line::Removed(a[i]));
            i += 1;
        } else {
            lines.push(Line::Added(b[j]));
            j += 1;
        }
    }
    lines.extend(old[old.len() - suffix..].iter().map(|line| Line::Same(line)));
    lines
}

/// The ranges of `lines` each hunk shows: every change with up to [`CONTEXT`] lines around it,
/// merged where they touch.
fn hunks(lines: &[Line<'_>]) -> Vec<(usize, usize)> {
    let mut hunks: Vec<(usize, usize)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if matches!(line, Line::Same(_)) {
            continue;
        }
        let start = index.saturating_sub(CONTEXT);
        let end = (index + 1 + CONTEXT).min(lines.len());
        match hunks.last_mut() {
            Some(last) if start <= last.1 => last.1 = end,
            _ => hunks.push((start, end)),
        }
    }
    hunks
}

fn write_hunk(out: &mut String, lines: &[Line<'_>], (start, end): (usize, usize)) {
    let counts = |range: &[Line<'_>]| {
        let old = range
            .iter()
            .filter(|line| !matches!(line, Line::Added(_)))
            .count();
        let new = range
            .iter()
            .filter(|line| !matches!(line, Line::Removed(_)))
            .count();
        (old, new)
    };
    let (old_before, new_before) = counts(&lines[..start]);
    let (old_count, new_count) = counts(&lines[start..end]);
    let position = |before: usize, count: usize| if count == 0 { before } else { before + 1 };
    // Writing to a String cannot fail.
    let _ = writeln!(
        out,
        "@@ -{},{old_count} +{},{new_count} @@",
        position(old_before, old_count),
        position(new_before, new_count)
    );
    for line in &lines[start..end] {
        let (mark, text) = match line {
            Line::Same(text) => (' ', text),
            Line::Removed(text) => ('-', text),
            Line::Added(text) => ('+', text),
        };
        out.push(mark);
        out.push_str(text);
        if !text.ends_with('\n') {
            out.push_str("\n\\ No newline at end of file\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_texts_have_no_diff() {
        assert_eq!(unified("a\n", "a\n"), "");
    }

    #[test]
    fn changes_show_with_three_lines_of_context() {
        let before = "1\n2\n3\n4\n5\n6\n7\n8\n9\n";
        let after = "1\n2\n3\n4\nfive\n6\n7\n8\n9\n";
        assert_eq!(
            unified(before, after),
            "--- a/unfold.toml\n+++ b/unfold.toml\n@@ -2,7 +2,7 @@\n 2\n 3\n 4\n-5\n+five\n 6\n 7\n 8\n"
        );
    }

    #[test]
    fn additions_to_an_empty_file_and_distant_changes() {
        assert_eq!(
            unified("", "[scope]\n"),
            "--- a/unfold.toml\n+++ b/unfold.toml\n@@ -0,0 +1,1 @@\n+[scope]\n"
        );
        let before: String = (1..=20).map(|n| format!("{n}\n")).collect();
        let after = before.replace("\n2\n", "\ntwo\n").replace("\n19\n", "\n");
        let diff = unified(&before, &after);
        assert_eq!(diff.matches("@@ -").count(), 2, "{diff}");
        assert!(diff.contains("-2\n+two\n"), "{diff}");
        assert!(
            diff.contains("@@ -16,5 +16,4 @@\n 16\n 17\n 18\n-19\n 20\n"),
            "{diff}"
        );
    }

    #[test]
    fn a_missing_final_newline_is_marked() {
        assert_eq!(
            unified("a", "b"),
            "--- a/unfold.toml\n+++ b/unfold.toml\n@@ -1,1 +1,1 @@\n-a\n\\ No newline at end of file\n+b\n\\ No newline at end of file\n"
        );
    }
}
