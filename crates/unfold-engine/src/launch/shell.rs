//! The POSIX shell subset: splitting a script into `&&`/`;` steps of decoded words.
//!
//! Words keep their quoting provenance because it changes meaning: `FOO=x node a.js` starts with
//! an assignment, but `"FOO=x" node a.js` runs a command named `FOO=x` (though `env` accepts a
//! quoted assignment as its argument).

use super::Refusal;

/// One decoded shell word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Word {
    /// The text after removing quotes and escapes.
    pub text: String,
    /// It contains an unquoted (or double-quoted) parameter expansion, glob character, or
    /// leading tilde, so its runtime value isn't its text.
    pub expansion: bool,
    /// Its written, unquoted prefix is `NAME=`.
    pub assignment: bool,
    /// It was written without any quote or escape.
    pub plain: bool,
    /// The word as written, quotes included, without line continuations.
    pub raw: String,
}

impl Word {
    /// The word's text, refusing it if the shell would expand it. `role` names what the word is
    /// used as, for the refusal message.
    pub fn literal(&self, role: &str) -> Result<&str, Refusal> {
        if self.expansion {
            return Err(Refusal::new(format!("Shell expansion in {role}")));
        }
        Ok(&self.text)
    }
}

/// `[A-Za-z_][A-Za-z0-9_]*=` at the start of `text`.
pub(super) fn is_assignment(text: &str) -> bool {
    let mut chars = text.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') {
        return false;
    }
    for c in chars {
        if c == '=' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return false;
        }
    }
    false
}

/// A character that starts a parameter expansion after `$`.
fn starts_parameter(c: Option<&char>) -> bool {
    c.is_some_and(|&c| c.is_ascii_alphanumeric() || "_{$*?!#@-".contains(c))
}

fn is_control(c: char) -> bool {
    matches!(c, '\0'..='\x1f' | '\x7f')
}

/// Splits `script` into steps of words. Fails on anything outside the subset: operators other
/// than `&&` and `;`, substitutions, comments, unquoted control characters, malformed quoting,
/// and empty steps.
pub(super) fn steps(script: &str) -> Result<Vec<Vec<Word>>, Refusal> {
    let chars: Vec<char> = script.chars().collect();
    let at = |index: usize| chars.get(index);
    let and_at = |index: usize| chars.get(index) == Some(&'&') && chars.get(index + 1) == Some(&'&');
    let mut steps = Vec::new();
    let mut step: Vec<Word> = Vec::new();
    let mut index = 0;
    let mut needs_step = false;
    while index < chars.len() {
        let c = chars[index];
        if c == ' ' || c == '\t' {
            index += 1;
            continue;
        }
        if c == ';' || and_at(index) {
            if step.is_empty() {
                return Err(Refusal::new("Empty shell step"));
            }
            steps.push(std::mem::take(&mut step));
            needs_step = c != ';';
            index += if c == ';' { 1 } else { 2 };
            continue;
        }
        let start = index;
        let mut text = String::new();
        let mut expansion = false;
        let mut plain = true;
        while index < chars.len() {
            let c = chars[index];
            if c == ' ' || c == '\t' || c == ';' || and_at(index) {
                break;
            }
            if is_control(c) {
                return Err(Refusal::new("Unsupported shell control character or newline"));
            }
            if "|&()<>`".contains(c) {
                return Err(Refusal::new(format!("Unsupported shell operator {c}")));
            }
            if c == '#' && text.is_empty() && plain {
                return Err(Refusal::new("Shell comments are unsupported"));
            }
            if c == '\\' {
                index += 1;
                match at(index) {
                    None => return Err(Refusal::new("Unfinished shell escape")),
                    Some('\0') => return Err(Refusal::new("NUL in shell word")),
                    // A line continuation disappears.
                    Some('\n') => {}
                    Some(&escaped) => {
                        text.push(escaped);
                        plain = false;
                    }
                }
                index += 1;
                continue;
            }
            if c == '\'' || c == '"' {
                plain = false;
                index += 1;
                while let Some(&q) = at(index).filter(|&&q| q != c) {
                    if q == '\0' {
                        return Err(Refusal::new("NUL in shell word"));
                    }
                    if c == '"' {
                        if q == '`' || (q == '$' && at(index + 1) == Some(&'(')) {
                            return Err(Refusal::new("Shell substitution is unsupported"));
                        }
                        if q == '$' && starts_parameter(at(index + 1)) {
                            expansion = true;
                        }
                        if q == '\\' && at(index + 1).is_some_and(|&next| "\"\\$`\n".contains(next)) {
                            index += 1;
                            if chars[index] != '\n' {
                                text.push(chars[index]);
                            }
                            index += 1;
                            continue;
                        }
                    }
                    text.push(q);
                    index += 1;
                }
                if index == chars.len() {
                    return Err(Refusal::new("Unterminated shell quote"));
                }
                index += 1;
                continue;
            }
            if c == '$' && at(index + 1) == Some(&'(') {
                return Err(Refusal::new("Shell substitution is unsupported"));
            }
            if c == '$' && at(index + 1).is_some_and(|&next| next == '\'' || next == '"') {
                return Err(Refusal::new("Non-POSIX shell quoting is unsupported"));
            }
            if c == '$' && starts_parameter(at(index + 1)) {
                expansion = true;
            }
            if "*?[".contains(c) || (c == '~' && text.is_empty() && plain) {
                expansion = true;
            }
            text.push(c);
            index += 1;
        }
        let raw: String = chars[start..index].iter().collect::<String>().replace("\\\n", "");
        // A line continuation alone creates no word; a quoted empty word does.
        if raw.is_empty() {
            continue;
        }
        step.push(Word {
            text,
            expansion,
            assignment: is_assignment(&raw),
            plain,
            raw,
        });
        needs_step = false;
    }
    if needs_step {
        return Err(Refusal::new("Missing step after &&"));
    }
    if !step.is_empty() {
        steps.push(step);
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(script: &str) -> Vec<Vec<String>> {
        steps(script)
            .unwrap()
            .into_iter()
            .map(|step| step.into_iter().map(|word| word.text).collect())
            .collect()
    }

    #[test]
    fn splits_steps_and_decodes_words() {
        assert_eq!(texts("a b && c;d"), [vec!["a", "b"], vec!["c"], vec!["d"]]);
        assert_eq!(texts("a ''"), [vec!["a", ""]]);
        assert_eq!(texts("a;"), [vec!["a"]]);
        assert!(texts(" \t").is_empty());
        assert_eq!(texts("a \\\n b"), [vec!["a", "b"]]);
    }

    #[test]
    fn records_provenance() {
        let words = steps("FOO=\"x\" 'BAR'=y $HOME ~/a b~ \"$X\" '$Y'")
            .unwrap()
            .remove(0);
        let flags: Vec<(bool, bool, bool)> = words
            .iter()
            .map(|word| (word.assignment, word.expansion, word.plain))
            .collect();
        assert_eq!(
            flags,
            [
                (true, false, false),
                (false, false, false),
                (false, true, true),
                (false, true, true),
                (false, false, true),
                (false, true, false),
                (false, false, false),
            ]
        );
        assert!(is_assignment("_A1=x"));
        assert!(!is_assignment("1A=x"));
        assert!(!is_assignment("A"));
        assert!(!is_assignment("A-B=x"));
    }
}
