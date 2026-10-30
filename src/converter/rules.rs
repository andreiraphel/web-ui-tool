//! User-defined pseudocode equivalents.
//!
//! A rules file is a small line-oriented format:
//!
//! ```text
//! term FUNCTION => PROCEDURE      # rename a generated whole word/phrase
//! call printf => OUTPUT {args}    # rewrite a call used as a statement
//! term TO => :=                   # rename a generated whole word/phrase
//! ```
//!
//! `term` rules only match whole words, so `term IF => WHEN` will change the
//! generated `IF` keyword but not the letters inside `DIFF`. `text` rules are
//! plain replacements. Neither kind touches string or character literals.

/// A mapping for a C function call used as a whole statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallRule {
    pub name: String,
    /// Replacement template. Supports `{args}` (all arguments) and `{1}`,
    /// `{2}`, ... (individual arguments). Without a placeholder the arguments
    /// are appended automatically.
    pub template: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rewrite {
    Term { from: String, to: String },
    Text { from: String, to: String },
}

#[derive(Debug, Clone, Default)]
pub struct Rules {
    pub calls: Vec<CallRule>,
    pub rewrites: Vec<Rewrite>,
}

impl Rules {
    /// Parse a rules file. Returns the valid rules plus human-readable errors.
    pub fn from_text(text: &str) -> (Self, Vec<String>) {
        let mut rules = Self::default();
        let mut errors = Vec::new();

        for (index, raw) in text.lines().enumerate() {
            let line_no = index + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((kind, body)) = line.split_once(char::is_whitespace) else {
                errors.push(format!(
                    "line {line_no}: expected 'term', 'call' or 'text', then '=>'"
                ));
                continue;
            };
            let Some((from, to)) = body.trim().split_once("=>") else {
                errors.push(format!(
                    "line {line_no}: missing '=>' between the source and the replacement"
                ));
                continue;
            };
            let from = from.trim();
            let to = to.trim();

            if from.is_empty() {
                errors.push(format!("line {line_no}: the source side is empty"));
                continue;
            }

            match kind {
                "call" => {
                    if !is_identifier(from) {
                        errors.push(format!(
                            "line {line_no}: '{from}' is not a valid function name"
                        ));
                        continue;
                    }
                    rules.calls.push(CallRule {
                        name: from.to_string(),
                        template: to.to_string(),
                    });
                }
                "term" => rules.rewrites.push(Rewrite::Term {
                    from: from.to_string(),
                    to: to.to_string(),
                }),
                "text" => rules.rewrites.push(Rewrite::Text {
                    from: from.to_string(),
                    to: to.to_string(),
                }),
                other => errors.push(format!(
                    "line {line_no}: unknown rule type '{other}' (use term, call or text)"
                )),
            }
        }

        (rules, errors)
    }

    pub fn call_rule(&self, name: &str) -> Option<&CallRule> {
        self.calls.iter().find(|rule| rule.name == name)
    }

    pub fn is_empty(&self) -> bool {
        self.calls.is_empty() && self.rewrites.is_empty()
    }

    pub fn rule_count(&self) -> usize {
        self.calls.len() + self.rewrites.len()
    }

    /// Apply `term`/`text` rewrites to generated pseudocode, leaving the
    /// contents of string and character literals untouched.
    pub fn apply(&self, code: &str) -> String {
        if self.rewrites.is_empty() {
            return code.to_string();
        }
        let mut out = String::with_capacity(code.len());
        for line in code.split_inclusive('\n') {
            out.push_str(&self.apply_line(line));
        }
        out
    }

    fn apply_line(&self, line: &str) -> String {
        let mut out = String::new();
        let mut plain = String::new();
        let mut chars = line.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '"' || c == '\'' {
                out.push_str(&self.apply_rewrites(&plain));
                plain.clear();
                out.push(c);
                let quote = c;
                let mut escaped = false;
                for ch in chars.by_ref() {
                    out.push(ch);
                    if escaped {
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == quote {
                        break;
                    }
                }
            } else {
                plain.push(c);
            }
        }

        out.push_str(&self.apply_rewrites(&plain));
        out
    }

    fn apply_rewrites(&self, text: &str) -> String {
        let mut current = text.to_string();
        for rewrite in &self.rewrites {
            current = match rewrite {
                Rewrite::Term { from, to } => replace_term(&current, from, to),
                Rewrite::Text { from, to } => current.replace(from, to),
            };
        }
        current
    }
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Replace whole-word/phrase occurrences of `from` in `haystack`.
fn replace_term(haystack: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return haystack.to_string();
    }

    let mut out = String::with_capacity(haystack.len());
    let mut last = 0usize;
    for (index, _) in haystack.match_indices(from) {
        if index < last {
            continue;
        }
        let end = index + from.len();
        let before_ok = haystack[..index]
            .chars()
            .next_back()
            .map(|c| !is_word_char(c))
            .unwrap_or(true);
        let after_ok = haystack[end..]
            .chars()
            .next()
            .map(|c| !is_word_char(c))
            .unwrap_or(true);
        if before_ok && after_ok {
            out.push_str(&haystack[last..index]);
            out.push_str(to);
            last = end;
        }
    }
    out.push_str(&haystack[last..]);
    out
}

pub const DEFAULT_RULES: &str = "\
# Custom pseudocode equivalents. One rule per line; '#' starts a comment.
#
# term FROM => TO
#   Rename a generated keyword or phrase. Matches whole words only.
#   Examples:
#     term FUNCTION => PROCEDURE
#     term PRINT => OUTPUT
#     term READ => INPUT
#
# call NAME => TEMPLATE
#   Rewrite a C function call that appears as a whole statement.
#   {args} expands to all arguments; {1}, {2}, ... expand to one argument.
#   Without a placeholder, the arguments are appended automatically.
#   Examples:
#     call printf => OUTPUT {args}
#     call scanf => INPUT {args}
#     call log_debug => LOG {1}
#
# text FROM => TO
#   Plain replacement outside string literals. Useful for symbols.
#   Examples:
#     term TO => :=
#     term NULL => NIL
#
# Rules are applied in the order they appear.
";
