//! A small, self-contained C-to-pseudocode translator.
//!
//! The translator is intentionally heuristic: it understands the C constructs
//! that usually matter for pseudocode (functions, control flow, declarations,
//! calls, common I/O functions) and degrades gracefully on everything else.

mod format;
mod parser;
mod rules;

pub use rules::{DEFAULT_RULES, Rules};

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Conversion {
    pub code: String,
    pub errors: Vec<ConvertError>,
}

#[allow(dead_code)]
pub fn convert(source: &str) -> Conversion {
    convert_with_rules(source, &Rules::default())
}

pub fn convert_with_rules(source: &str, rules: &Rules) -> Conversion {
    let source = expand_tabs(source);
    let mut errors = Vec::new();
    let tokens = lex(&source, &mut errors);
    let mut parser = parser::Parser::new(tokens, errors, rules.clone());
    parser.parse_unit();
    let code = expand_tabs(&parser.finish());
    let mut errors = std::mem::take(&mut parser.errors);
    errors.sort_by_key(|e| e.line);
    Conversion {
        code: expand_tabs(&rules.apply(&code)),
        errors,
    }
}

fn expand_tabs(text: &str) -> String {
    if text.contains('\t') {
        text.replace('\t', "    ")
    } else {
        text.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tokens and lexer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Ident,
    Number,
    Str,
    Char,
    Punct,
    Preproc,
    Comment,
}

#[derive(Debug, Clone)]
pub(crate) struct Token {
    pub kind: Kind,
    pub text: String,
    pub line: usize,
    pub indent: usize,
}

const MULTI_OPS: &[&str] = &[
    "<<=", ">>=", "...", "->", "++", "--", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+=",
    "-=", "*=", "/=", "%=", "&=", "|=", "^=", "##",
];

const SINGLE_PUNCT: &str = "{}()[];,.:?~!+-*/%=<>&|^#";

pub(crate) const TYPE_WORDS: &[&str] = &[
    "void",
    "char",
    "short",
    "int",
    "long",
    "float",
    "double",
    "signed",
    "unsigned",
    "_Bool",
    "bool",
    "size_t",
    "ssize_t",
    "ptrdiff_t",
    "wchar_t",
    "int8_t",
    "int16_t",
    "int32_t",
    "int64_t",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "intptr_t",
    "uintptr_t",
    "FILE",
];

pub(crate) const QUALIFIERS: &[&str] = &[
    "const",
    "volatile",
    "restrict",
    "static",
    "extern",
    "register",
    "auto",
    "inline",
    "_Atomic",
    "thread_local",
    "_Noreturn",
];

pub(crate) const STORAGE_WORDS: &[&str] = &[
    "static",
    "extern",
    "register",
    "auto",
    "inline",
    "_Noreturn",
    "thread_local",
];

pub(crate) const ASSIGN_OPS: &[&str] = &[
    "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=",
];

pub(crate) const BINARY_OPS: &[&str] = &[
    "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", "<", ">", "<=", ">=", "!=",
    "<<", ">>", "/", "%", "|", "^", "?", ":",
];

pub(crate) fn is_type_word(s: &str) -> bool {
    TYPE_WORDS.contains(&s)
}

pub(crate) fn is_qualifier(s: &str) -> bool {
    QUALIFIERS.contains(&s)
}

pub(crate) fn is_struct_like(s: &str) -> bool {
    matches!(s, "struct" | "union" | "enum")
}

fn lex(source: &str, errors: &mut Vec<ConvertError>) -> Vec<Token> {
    let chars: Vec<char> = source.chars().collect();
    let n = chars.len();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut line_has_token = false;
    let mut line_indent = 0usize;
    let mut tokens = Vec::new();

    while i < n {
        let c = chars[i];

        if c == '\n' {
            line += 1;
            line_has_token = false;
            line_indent = 0;
            i += 1;
            continue;
        }
        if c == '\r' {
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            if c == ' ' && !line_has_token {
                line_indent += 1;
            }
            i += 1;
            continue;
        }

        // Preprocessor directives are only recognised at the start of a line.
        if c == '#' && !line_has_token {
            let start_line = line;
            let mut text = String::from('#');
            i += 1;
            while i < n {
                let ch = chars[i];
                if ch == '\n' {
                    if text.trim_end().ends_with('\\') {
                        if let Some(p) = text.rfind('\\') {
                            text.truncate(p);
                        }
                        text.push(' ');
                        line += 1;
                        i += 1;
                        continue;
                    }
                    break;
                }
                text.push(ch);
                i += 1;
            }
            tokens.push(Token {
                kind: Kind::Preproc,
                text: text.trim().to_string(),
                line: start_line,
                indent: line_indent,
            });
            line_has_token = true;
            continue;
        }

        // Comments are preserved as tokens so generated pseudocode keeps them.
        if c == '/' && i + 1 < n && chars[i + 1] == '/' {
            let start = i;
            let start_line = line;
            i += 2;
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            tokens.push(Token {
                kind: Kind::Comment,
                text: chars[start..i].iter().collect(),
                line: start_line,
                indent: line_indent,
            });
            line_has_token = true;
            continue;
        }
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            let start = i;
            let start_line = line;
            i += 2;
            let mut closed = false;
            while i < n {
                if chars[i] == '*' && i + 1 < n && chars[i + 1] == '/' {
                    i += 2;
                    closed = true;
                    break;
                }
                if chars[i] == '\n' {
                    line += 1;
                    line_indent = 0;
                }
                i += 1;
            }
            if !closed {
                errors.push(ConvertError {
                    line: start_line,
                    message: "unterminated block comment".into(),
                });
            }
            tokens.push(Token {
                kind: Kind::Comment,
                text: chars[start..i].iter().collect(),
                line: start_line,
                indent: line_indent,
            });
            line_has_token = true;
            continue;
        }

        // String / character literals.
        if c == '"' || c == '\'' {
            let quote = c;
            let start = i;
            let start_line = line;
            i += 1;
            let mut closed = false;
            while i < n {
                let ch = chars[i];
                if ch == '\\' {
                    if i + 1 < n && chars[i + 1] == '\n' {
                        line += 1;
                        line_indent = 0;
                    }
                    i += 2;
                    continue;
                }
                if ch == '\n' {
                    break;
                }
                if ch == quote {
                    i += 1;
                    closed = true;
                    break;
                }
                i += 1;
            }
            if !closed {
                errors.push(ConvertError {
                    line: start_line,
                    message: "unterminated literal".into(),
                });
            }
            tokens.push(Token {
                kind: if quote == '"' { Kind::Str } else { Kind::Char },
                text: chars[start..i.min(n)].iter().collect(),
                line: start_line,
                indent: line_indent,
            });
            line_has_token = true;
            continue;
        }

        // Identifiers.
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(Token {
                kind: Kind::Ident,
                text: chars[start..i].iter().collect(),
                line,
                indent: line_indent,
            });
            line_has_token = true;
            continue;
        }

        // Numbers (decimal, hex, floats, suffixes).
        if c.is_ascii_digit() || (c == '.' && i + 1 < n && chars[i + 1].is_ascii_digit()) {
            let start = i;
            while i < n {
                let ch = chars[i];
                if ch.is_alphanumeric()
                    || ch == '.'
                    || ch == '_'
                    || ((ch == '+' || ch == '-')
                        && i > start
                        && matches!(chars[i - 1], 'e' | 'E' | 'p' | 'P'))
                {
                    i += 1;
                } else {
                    break;
                }
            }
            tokens.push(Token {
                kind: Kind::Number,
                text: chars[start..i].iter().collect(),
                line,
                indent: line_indent,
            });
            line_has_token = true;
            continue;
        }

        // Multi-character operators.
        let mut matched = None;
        for op in MULTI_OPS {
            if starts_with_at(&chars, i, op) {
                matched = Some(*op);
                break;
            }
        }
        if let Some(op) = matched {
            tokens.push(Token {
                kind: Kind::Punct,
                text: op.to_string(),
                line,
                indent: line_indent,
            });
            i += op.chars().count();
            line_has_token = true;
            continue;
        }

        if SINGLE_PUNCT.contains(c) {
            tokens.push(Token {
                kind: Kind::Punct,
                text: c.to_string(),
                line,
                indent: line_indent,
            });
            i += 1;
            line_has_token = true;
            continue;
        }

        errors.push(ConvertError {
            line,
            message: format!("unexpected character {c:?}"),
        });
        i += 1;
    }

    tokens
}

fn starts_with_at(chars: &[char], i: usize, pat: &str) -> bool {
    for (k, pc) in (i..).zip(pat.chars()) {
        if k >= chars.len() || chars[k] != pc {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests;
