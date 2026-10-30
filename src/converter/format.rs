//! Token-level formatting: expressions, declarators and statement helpers.

use super::rules::{CallRule, Rules};
use super::{
    ASSIGN_OPS, BINARY_OPS, Kind, STORAGE_WORDS, Token, is_qualifier, is_struct_like, is_type_word,
};

pub(crate) fn matching_close(
    tokens: &[Token],
    open_idx: usize,
    open: &str,
    close: &str,
) -> Option<usize> {
    let mut depth = 0i32;
    for (i, t) in tokens.iter().enumerate().skip(open_idx) {
        if t.text == open {
            depth += 1;
        } else if t.text == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

pub(crate) fn skip_balanced(tokens: &[Token], open_idx: usize, open: &str, close: &str) -> usize {
    matching_close(tokens, open_idx, open, close)
        .map(|i| i + 1)
        .unwrap_or(tokens.len())
}

pub(crate) fn split_top_level<'a>(tokens: &'a [Token], sep: &str) -> Vec<&'a [Token]> {
    let mut result = Vec::new();
    let mut start = 0usize;
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    for i in 0..tokens.len() {
        let t = tokens[i].text.as_str();
        if t == sep && paren == 0 && bracket == 0 && brace == 0 {
            result.push(&tokens[start..i]);
            start = i + 1;
            continue;
        }
        match t {
            "(" => paren += 1,
            ")" => paren -= 1,
            "[" => bracket += 1,
            "]" => bracket -= 1,
            "{" => brace += 1,
            "}" => brace -= 1,
            _ => {}
        }
    }
    if start < tokens.len() {
        result.push(&tokens[start..]);
    }
    result
}

pub(crate) fn find_top_level(tokens: &[Token], needle: &str) -> Option<usize> {
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    for (i, t) in tokens.iter().enumerate() {
        let text = t.text.as_str();
        if text == needle && paren == 0 && bracket == 0 && brace == 0 {
            return Some(i);
        }
        match text {
            "(" => paren += 1,
            ")" => paren -= 1,
            "[" => bracket += 1,
            "]" => bracket -= 1,
            "{" => brace += 1,
            "}" => brace -= 1,
            _ => {}
        }
    }
    None
}

/// Index of the first declarator in a declaration's token list (skips the type).
pub(crate) fn declarator_start(tokens: &[Token]) -> usize {
    let mut i = 0usize;
    while i < tokens.len() && is_qualifier(&tokens[i].text) {
        i += 1;
    }
    if i < tokens.len() {
        if is_struct_like(&tokens[i].text) {
            i += 1;
            if tokens.get(i).map(|t| t.kind) == Some(Kind::Ident) {
                i += 1;
            }
        } else if is_type_word(&tokens[i].text) {
            while i < tokens.len()
                && (is_type_word(&tokens[i].text) || is_qualifier(&tokens[i].text))
            {
                i += 1;
            }
        } else if tokens[i].kind == Kind::Ident {
            i += 1;
        }
    }
    while i < tokens.len() && (tokens[i].text == "*" || is_qualifier(&tokens[i].text)) {
        i += 1;
    }
    i
}

pub(crate) fn format_type(tokens: &[Token]) -> String {
    let kept: Vec<&str> = tokens
        .iter()
        .map(|t| t.text.as_str())
        .filter(|t| !STORAGE_WORDS.contains(t))
        .collect();
    if kept.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for t in kept {
        if t == "*" {
            if !out.is_empty() && !out.ends_with('*') {
                out.push(' ');
            }
            out.push('*');
        } else {
            if !out.is_empty() && !out.ends_with('*') && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push_str(t);
        }
    }
    out
}

pub(crate) fn join_type_declaration(type_name: &str, decl: &str) -> String {
    if type_name.is_empty() {
        decl.to_string()
    } else if type_name.ends_with('*') {
        format!("{type_name}{decl}")
    } else {
        format!("{type_name} {decl}")
    }
}

pub(crate) fn format_params(tokens: &[Token]) -> String {
    let mut names = Vec::new();
    for part in split_top_level(tokens, ",") {
        if part.is_empty() {
            continue;
        }
        if part.len() == 1 && part[0].text == "void" {
            continue;
        }
        if part.iter().any(|t| t.text == "...") {
            names.push("...".to_string());
            continue;
        }
        let name = part
            .iter()
            .rev()
            .find(|t| t.kind == Kind::Ident && !is_type_word(&t.text) && !is_qualifier(&t.text))
            .map(|t| t.text.clone());
        if let Some(name) = name {
            names.push(name);
        }
    }
    names.join(", ")
}

pub(crate) fn format_declarators(tokens: &[Token]) -> Vec<String> {
    let mut parts = Vec::new();
    let mut i = 0usize;

    while i < tokens.len() {
        while i < tokens.len()
            && (is_qualifier(&tokens[i].text) || tokens[i].text == "*" || tokens[i].text == ",")
        {
            i += 1;
        }
        if i >= tokens.len() {
            break;
        }

        let name;
        if tokens[i].text == "(" && tokens.get(i + 1).map(|t| t.text.as_str()) == Some("*") {
            let ident = match tokens.get(i + 2) {
                Some(t) if t.kind == Kind::Ident => t.text.clone(),
                _ => break,
            };
            i += 3;
            if tokens.get(i).map(|t| t.text.as_str()) == Some(")") {
                i += 1;
            }
            if tokens.get(i).map(|t| t.text.as_str()) == Some("(") {
                i = skip_balanced(tokens, i, "(", ")");
            }
            name = ident;
        } else if tokens[i].kind == Kind::Ident {
            name = tokens[i].text.clone();
            i += 1;
        } else {
            break;
        }

        let mut piece = name;
        while i < tokens.len() && tokens[i].text == "[" {
            let close =
                matching_close(tokens, i, "[", "]").unwrap_or(tokens.len().saturating_sub(1));
            let inner = if close > i {
                format_expr(&tokens[i + 1..close])
            } else {
                String::new()
            };
            piece.push('[');
            piece.push_str(&inner);
            piece.push(']');
            i = (close + 1).min(tokens.len());
        }

        if i < tokens.len() && tokens[i].text == ":" {
            while i < tokens.len() && tokens[i].text != "," {
                i += 1;
            }
        }

        if i < tokens.len() && tokens[i].text == "=" {
            i += 1;
            let start = i;
            let mut paren = 0i32;
            let mut bracket = 0i32;
            let mut brace = 0i32;
            while i < tokens.len() {
                match tokens[i].text.as_str() {
                    "(" => paren += 1,
                    ")" => paren -= 1,
                    "[" => bracket += 1,
                    "]" => bracket -= 1,
                    "{" => brace += 1,
                    "}" => brace -= 1,
                    "," if paren == 0 && bracket == 0 && brace == 0 => break,
                    _ => {}
                }
                i += 1;
            }
            let value = format_expr(&tokens[start..i]);
            piece.push_str(" TO ");
            piece.push_str(&value);
        }

        parts.push(piece);

        if i < tokens.len() && tokens[i].text == "," {
            i += 1;
            continue;
        }
        break;
    }

    parts
}

pub(crate) fn format_for_part(tokens: &[Token], rules: &Rules) -> String {
    if tokens.is_empty() {
        return String::new();
    }
    let start = declarator_start(tokens);
    let is_decl = start > 0
        && start < tokens.len()
        && (is_type_word(&tokens[0].text) || is_qualifier(&tokens[0].text));
    if is_decl {
        let type_name = format_type(&tokens[..start]);
        let parts = format_declarators(&tokens[start..]);
        if parts.is_empty() {
            return String::new();
        }
        return format!(
            "INIT {}",
            join_type_declaration(&type_name, &parts.join(", "))
        );
    }
    split_top_level(tokens, ",")
        .into_iter()
        .map(|part| format_inline(part, false, rules).unwrap_or_else(|| format_expr(part)))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn split_assignment(tokens: &[Token]) -> Option<(&[Token], &str, &[Token])> {
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    for (i, t) in tokens.iter().enumerate() {
        if paren == 0 && bracket == 0 && brace == 0 && ASSIGN_OPS.contains(&t.text.as_str()) {
            if i == 0 || i + 1 >= tokens.len() {
                return None;
            }
            return Some((&tokens[..i], t.text.as_str(), &tokens[i + 1..]));
        }
        match t.text.as_str() {
            "(" => paren += 1,
            ")" => paren -= 1,
            "[" => bracket += 1,
            "]" => bracket -= 1,
            "{" => brace += 1,
            "}" => brace -= 1,
            _ => {}
        }
    }
    None
}

fn is_lvalue(tokens: &[Token]) -> bool {
    if tokens.is_empty() || tokens[0].kind != Kind::Ident {
        return false;
    }
    let mut i = 1usize;
    while i < tokens.len() {
        match tokens[i].text.as_str() {
            "[" => {
                let Some(close) = matching_close(tokens, i, "[", "]") else {
                    return false;
                };
                i = close + 1;
            }
            "." | "->" => {
                i += 1;
                if tokens.get(i).map(|t| t.kind) != Some(Kind::Ident) {
                    return false;
                }
                i += 1;
            }
            _ => return false,
        }
    }
    true
}

fn format_increment(tokens: &[Token]) -> Option<String> {
    if tokens.len() < 2 {
        return None;
    }
    let last = tokens.last()?.text.as_str();
    if (last == "++" || last == "--") && is_lvalue(&tokens[..tokens.len() - 1]) {
        let lhs = format_expr(&tokens[..tokens.len() - 1]);
        let word = if last == "++" {
            "INCREMENT"
        } else {
            "DECREMENT"
        };
        return Some(format!("{word} {lhs}"));
    }
    let first = tokens[0].text.as_str();
    if (first == "++" || first == "--") && is_lvalue(&tokens[1..]) {
        let lhs = format_expr(&tokens[1..]);
        let word = if first == "++" {
            "INCREMENT"
        } else {
            "DECREMENT"
        };
        return Some(format!("{word} {lhs}"));
    }
    None
}

fn call_parts(tokens: &[Token]) -> Option<(&str, &[Token])> {
    if tokens.len() < 3 {
        return None;
    }
    if tokens[0].kind != Kind::Ident || tokens[1].text != "(" {
        return None;
    }
    let close = matching_close(tokens, 1, "(", ")")?;
    if close != tokens.len() - 1 {
        return None;
    }
    Some((tokens[0].text.as_str(), &tokens[2..close]))
}

pub(crate) fn clean_string_literal(raw: &str) -> String {
    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push_str("\\n"),
            Some('r') => out.push_str("\\r"),
            Some('0') => out.push_str("\\0"),
            Some('t') => out.push_str("    "),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn printable_expr(tokens: &[Token]) -> String {
    if tokens.len() == 1 && tokens[0].kind == Kind::Str {
        format!("\"{}\"", clean_string_literal(&tokens[0].text))
    } else {
        format_expr(tokens)
    }
}

fn format_special_call(tokens: &[Token], rules: &Rules) -> Option<String> {
    let (name, arg_tokens) = call_parts(tokens)?;
    let args = split_top_level(arg_tokens, ",");
    if let Some(rule) = rules.call_rule(name) {
        return Some(apply_call_template(rule, &args));
    }
    match name {
        "printf" => {
            let mut parts = Vec::new();
            let mut start = 0usize;
            if let Some(first) = args.first()
                && first.len() == 1
                && first[0].kind == Kind::Str
            {
                let cleaned = clean_string_literal(&first[0].text);
                if !cleaned.is_empty() {
                    parts.push(format!("\"{cleaned}\""));
                }
                start = 1;
            }
            for arg in args.iter().skip(start) {
                let formatted = format_expr(arg);
                if !formatted.is_empty() {
                    parts.push(formatted);
                }
            }
            Some(if parts.is_empty() {
                "PRINT".to_string()
            } else {
                format!("PRINT {}", parts.join(", "))
            })
        }
        "scanf" => {
            let mut start = 0usize;
            if args
                .first()
                .map(|a| a.len() == 1 && a[0].kind == Kind::Str)
                .unwrap_or(false)
            {
                start = 1;
            }
            let mut targets = Vec::new();
            for arg in args.iter().skip(start) {
                let mut formatted = format_expr(arg);
                if let Some(rest) = formatted.strip_prefix('&') {
                    formatted = rest.trim_start().to_string();
                }
                if !formatted.is_empty() {
                    targets.push(formatted);
                }
            }
            Some(if targets.is_empty() {
                "READ".to_string()
            } else {
                format!("READ {}", targets.join(", "))
            })
        }
        "gets" => {
            let target = args.first().map(|a| format_expr(a)).unwrap_or_default();
            Some(if target.is_empty() {
                "READ".to_string()
            } else {
                format!("READ {target}")
            })
        }
        "puts" => {
            let arg = args.first().map(|a| printable_expr(a)).unwrap_or_default();
            Some(if arg.is_empty() {
                "PRINT".to_string()
            } else {
                format!("PRINT {arg}")
            })
        }
        "putchar" => {
            let arg = args.first().map(|a| format_expr(a)).unwrap_or_default();
            Some(if arg.is_empty() {
                "PRINT".to_string()
            } else {
                format!("PRINT {arg}")
            })
        }
        "getchar" => Some("READ CHARACTER".to_string()),
        "assert" => {
            let arg = args.first().map(|a| format_expr(a)).unwrap_or_default();
            Some(format!("ASSERT {arg}"))
        }
        "exit" => {
            let arg = args.first().map(|a| format_expr(a)).unwrap_or_default();
            Some(if arg.is_empty() {
                "EXIT".to_string()
            } else {
                format!("EXIT {arg}")
            })
        }
        "free" => {
            let arg = args.first().map(|a| format_expr(a)).unwrap_or_default();
            Some(if arg.is_empty() {
                "FREE".to_string()
            } else {
                format!("FREE {arg}")
            })
        }
        _ => None,
    }
}

fn apply_call_template(rule: &CallRule, args: &[&[Token]]) -> String {
    let formatted_args: Vec<String> = args.iter().map(|arg| printable_expr(arg)).collect();
    let all_args = formatted_args.join(", ");

    if !rule.template.contains('{') {
        if all_args.is_empty() {
            return rule.template.clone();
        }
        return format!("{} {}", rule.template, all_args);
    }

    let mut out = String::new();
    let mut chars = rule.template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '{' {
            out.push(c);
            continue;
        }
        let mut inner = String::new();
        let mut closed = false;
        for ch in chars.by_ref() {
            if ch == '}' {
                closed = true;
                break;
            }
            inner.push(ch);
        }
        if !closed {
            out.push('{');
            out.push_str(&inner);
            continue;
        }
        match inner.as_str() {
            "args" | "*" => out.push_str(&all_args),
            _ => {
                if let Ok(index) = inner.parse::<usize>() {
                    if index >= 1 && index <= formatted_args.len() {
                        out.push_str(&formatted_args[index - 1]);
                    }
                } else {
                    out.push('{');
                    out.push_str(&inner);
                    out.push('}');
                }
            }
        }
    }
    out
}

pub(crate) fn format_inline(tokens: &[Token], call_prefix: bool, rules: &Rules) -> Option<String> {
    if tokens.is_empty() {
        return None;
    }
    if let Some((lhs, op, rhs)) = split_assignment(tokens) {
        let lhs = format_expr(lhs);
        if op == "=" {
            let rhs = format_inline(rhs, false, rules).unwrap_or_else(|| format_expr(rhs));
            return Some(format!("SET {lhs} TO {rhs}"));
        }
        let rhs = format_expr(rhs);
        match op {
            "+=" => return Some(format!("INCREMENT {lhs} BY {rhs}")),
            "-=" => return Some(format!("DECREMENT {lhs} BY {rhs}")),
            "*=" => return Some(format!("MULTIPLY {lhs} BY {rhs}")),
            "/=" => return Some(format!("DIVIDE {lhs} BY {rhs}")),
            "%=" => return Some(format!("SET {lhs} TO {lhs} % {rhs}")),
            _ => {
                if let Some(base) = op.strip_suffix('=') {
                    return Some(format!("SET {lhs} TO {lhs} {base} {rhs}"));
                }
            }
        }
    }
    if let Some(line) = format_increment(tokens) {
        return Some(line);
    }
    if let Some(line) = format_special_call(tokens, rules) {
        return Some(line);
    }
    if call_prefix && let Some((name, args)) = call_parts(tokens) {
        let args = format_expr(args);
        return Some(format!("CALL {name}({args})"));
    }
    let formatted = format_expr(tokens);
    if formatted.is_empty() {
        None
    } else {
        Some(formatted)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderKind {
    Word,
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    OpenBrace,
    CloseBrace,
    Dot,
    Comma,
    Binary,
    Unary,
    UnaryWord,
    Postfix,
    Prefix,
}

fn render_kind(mapped: &str, prev_operand: bool) -> RenderKind {
    match mapped {
        "AND" | "OR" => RenderKind::Binary,
        "NOT" => RenderKind::UnaryWord,
        "(" => RenderKind::OpenParen,
        ")" => RenderKind::CloseParen,
        "[" => RenderKind::OpenBracket,
        "]" => RenderKind::CloseBracket,
        "{" => RenderKind::OpenBrace,
        "}" => RenderKind::CloseBrace,
        "," => RenderKind::Comma,
        "." => RenderKind::Dot,
        "?" | ":" => RenderKind::Binary,
        "++" | "--" => {
            if prev_operand {
                RenderKind::Postfix
            } else {
                RenderKind::Prefix
            }
        }
        "+" | "-" | "*" | "&" => {
            if prev_operand {
                RenderKind::Binary
            } else {
                RenderKind::Unary
            }
        }
        "~" | "!" => RenderKind::Unary,
        _ if BINARY_OPS.contains(&mapped) => RenderKind::Binary,
        _ => RenderKind::Word,
    }
}

fn is_cast_type(tokens: &[Token]) -> bool {
    if tokens.is_empty() {
        return false;
    }
    if !tokens
        .iter()
        .all(|t| t.kind == Kind::Ident || t.text == "*" || is_qualifier(&t.text))
    {
        return false;
    }
    let first = tokens[0].text.as_str();
    let has_star = tokens.iter().any(|t| t.text == "*");
    is_type_word(first)
        || is_qualifier(first)
        || is_struct_like(first)
        || (has_star && tokens[0].kind == Kind::Ident)
}

fn can_start_operand(token: &Token) -> bool {
    matches!(
        token.kind,
        Kind::Ident | Kind::Number | Kind::Str | Kind::Char
    ) || matches!(
        token.text.as_str(),
        "(" | "&" | "*" | "-" | "+" | "!" | "~" | "++" | "--"
    )
}

/// Mark closing parentheses of C-style casts, plus the tokens inside them.
fn cast_flags(tokens: &[Token]) -> (Vec<bool>, Vec<bool>) {
    let mut closes = vec![false; tokens.len()];
    let mut inner = vec![false; tokens.len()];
    for i in 0..tokens.len() {
        if tokens[i].text != "(" {
            continue;
        }
        if i > 0 && matches!(tokens[i - 1].text.as_str(), "sizeof" | "_Alignof") {
            continue;
        }
        let Some(close) = matching_close(tokens, i, "(", ")") else {
            continue;
        };
        if close + 1 < tokens.len()
            && is_cast_type(&tokens[i + 1..close])
            && can_start_operand(&tokens[close + 1])
        {
            closes[close] = true;
            for flag in &mut inner[i + 1..close] {
                *flag = true;
            }
        }
    }
    (closes, inner)
}

pub(crate) fn format_expr(tokens: &[Token]) -> String {
    if tokens.is_empty() {
        return String::new();
    }

    struct Rendered {
        text: String,
        kind: RenderKind,
    }

    let (casts, cast_inner) = cast_flags(tokens);
    let mut rendered: Vec<Rendered> = Vec::with_capacity(tokens.len());
    for (index, t) in tokens.iter().enumerate() {
        let mapped = match t.text.as_str() {
            "==" => "IS EQUAL TO",
            "!=" => "IS NOT EQUAL TO",
            ">=" => "IS GREATER THAN OR EQUAL TO",
            "<=" => "IS LESS THAN OR EQUAL TO",
            ">" => "IS GREATER THAN",
            "<" => "IS LESS THAN",
            "!" => "NOT",
            "&&" => "AND",
            "||" => "OR",
            other => other,
        };
        let prev_was_cast = index > 0
            && casts[index - 1]
            && rendered.last().map(|r| r.kind) == Some(RenderKind::CloseParen);
        let prev_operand = !prev_was_cast
            && matches!(
                rendered.last().map(|r| r.kind),
                Some(RenderKind::Word)
                    | Some(RenderKind::CloseParen)
                    | Some(RenderKind::CloseBracket)
                    | Some(RenderKind::Postfix)
            );
        let kind = match t.text.as_str() {
            "==" | "!=" | ">=" | "<=" | ">" | "<" | "&&" | "||" => RenderKind::Binary,
            "->" => RenderKind::Dot,
            _ => render_kind(mapped, prev_operand),
        };
        rendered.push(Rendered {
            text: mapped.to_string(),
            kind,
        });
    }

    let mut out = String::new();
    let mut pending_space = false;
    let mut prev_kind: Option<RenderKind> = None;

    for (index, r) in rendered.iter().enumerate() {
        if index > 0 && tokens[index].line > tokens[index - 1].line {
            while out.ends_with(' ') {
                out.pop();
            }
            let gap = tokens[index].line - tokens[index - 1].line;
            for _ in 0..gap {
                out.push('\n');
            }
            out.push_str(&" ".repeat(tokens[index].indent));
            pending_space = false;
            prev_kind = None;
        }
        let prev_was_cast =
            index > 0 && casts[index - 1] && prev_kind == Some(RenderKind::CloseParen);
        let attach_pointer =
            r.text == "*" && cast_inner[index] && index > 0 && tokens[index - 1].text == "*";
        let space = if out.is_empty() || attach_pointer {
            false
        } else {
            match r.kind {
                RenderKind::CloseParen
                | RenderKind::CloseBracket
                | RenderKind::Comma
                | RenderKind::Dot
                | RenderKind::Postfix
                | RenderKind::OpenBracket => false,
                RenderKind::OpenParen => {
                    pending_space
                        && matches!(
                            prev_kind,
                            Some(RenderKind::Binary)
                                | Some(RenderKind::Comma)
                                | Some(RenderKind::UnaryWord)
                                | Some(RenderKind::OpenBrace)
                        )
                }
                RenderKind::OpenBrace => true,
                RenderKind::CloseBrace => !matches!(prev_kind, Some(RenderKind::OpenBrace)),
                RenderKind::Word => {
                    !prev_was_cast
                        && (pending_space
                            || matches!(
                                prev_kind,
                                Some(RenderKind::Word)
                                    | Some(RenderKind::CloseParen)
                                    | Some(RenderKind::CloseBracket)
                                    | Some(RenderKind::Postfix)
                            ))
                }
                RenderKind::UnaryWord => {
                    !prev_was_cast
                        && (pending_space
                            || matches!(
                                prev_kind,
                                Some(RenderKind::Word)
                                    | Some(RenderKind::CloseParen)
                                    | Some(RenderKind::CloseBracket)
                            ))
                }
                RenderKind::Unary | RenderKind::Prefix => pending_space,
                RenderKind::Binary => true,
            }
        };
        if space && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push_str(&r.text);
        pending_space = matches!(
            r.kind,
            RenderKind::Binary | RenderKind::Comma | RenderKind::UnaryWord | RenderKind::OpenBrace
        );
        prev_kind = Some(r.kind);
    }

    out
}
