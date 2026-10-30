//! Recursive-descent-ish statement translator for a practical subset of C.

use super::format::{
    declarator_start, find_top_level, format_declarators, format_expr, format_for_part,
    format_inline, format_params, format_type, join_type_declaration, matching_close,
};
use super::rules::Rules;
use super::{ConvertError, Kind, Token, is_qualifier, is_struct_like, is_type_word};

fn uppercase_defined(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("defined") {
        let before_word = rest[..pos]
            .chars()
            .next_back()
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false);
        let after = &rest[pos + "defined".len()..];
        let after_word = after
            .chars()
            .next()
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false);
        out.push_str(&rest[..pos]);
        if before_word || after_word {
            out.push_str("defined");
        } else {
            out.push_str("DEFINED");
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BreakCtx {
    Loop,
    Switch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopKind {
    FunctionDef,
    Prototype,
    Declaration,
    StructDef,
    Unknown,
}

pub(crate) struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    out: Vec<String>,
    indent: usize,
    pub(crate) errors: Vec<ConvertError>,
    break_stack: Vec<BreakCtx>,
    rules: Rules,
    source_indent: usize,
    force_structural: bool,
}

impl Parser {
    pub(crate) fn new(tokens: Vec<Token>, errors: Vec<ConvertError>, rules: Rules) -> Self {
        Self {
            tokens,
            pos: 0,
            out: Vec::new(),
            indent: 0,
            errors,
            break_stack: Vec::new(),
            rules,
            source_indent: 0,
            force_structural: false,
        }
    }

    pub(crate) fn finish(&mut self) -> String {
        while self.out.last().map(|s| s.is_empty()).unwrap_or(false) {
            self.out.pop();
        }
        if self.out.is_empty() {
            String::new()
        } else {
            let mut s = self.out.join("\n");
            s.push('\n');
            s
        }
    }

    // -- small helpers ------------------------------------------------------

    fn eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_text(&self) -> &str {
        self.peek().map(|t| t.text.as_str()).unwrap_or("")
    }

    fn text_at(&self, index: usize) -> &str {
        self.tokens
            .get(index)
            .map(|t| t.text.as_str())
            .unwrap_or("")
    }

    fn check(&self, text: &str) -> bool {
        self.peek_text() == text
    }

    fn bump(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, text: &str) -> bool {
        if self.check(text) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, text: &str) -> bool {
        if self.eat(text) {
            true
        } else {
            let line = self
                .peek()
                .map(|t| t.line)
                .unwrap_or_else(|| self.last_line());
            self.errors.push(ConvertError {
                line,
                message: format!("expected '{text}'"),
            });
            false
        }
    }

    fn last_line(&self) -> usize {
        self.tokens.last().map(|t| t.line).unwrap_or(1)
    }

    fn error(&mut self, message: impl Into<String>) {
        let line = self
            .peek()
            .map(|t| t.line)
            .unwrap_or_else(|| self.last_line());
        self.errors.push(ConvertError {
            line,
            message: message.into(),
        });
    }

    fn emit(&mut self, text: impl Into<String>) {
        let text = text.into();
        let structural = self.indent * 4;
        let spaces = if self.force_structural {
            structural
        } else {
            structural.max(self.source_indent)
        };
        let prefix = " ".repeat(spaces);
        self.out.push(format!("{prefix}{text}"));
    }

    fn blank(&mut self) {
        if self.out.last().map(|s| s.is_empty()).unwrap_or(true) {
            return;
        }
        self.out.push(String::new());
    }

    fn peek_is_comment(&self) -> bool {
        self.peek().map(|t| t.kind) == Some(Kind::Comment)
    }

    fn emit_comment(&mut self) {
        if let Some(token) = self.bump() {
            self.source_indent = token.indent;
            for line in token.text.lines() {
                self.emit(line.trim_end());
            }
        }
    }

    fn emit_comments(&mut self) {
        while self.peek_is_comment() {
            self.emit_comment();
        }
    }

    fn preserve_blank_lines(&mut self) {
        if self.pos == 0 || self.pos >= self.tokens.len() {
            return;
        }
        let previous = &self.tokens[self.pos - 1];
        let previous_end = previous.line + previous.text.matches('\n').count();
        let desired = self.tokens[self.pos].line.saturating_sub(previous_end + 1);
        if desired == 0 {
            return;
        }

        let mut existing = 0usize;
        for line in self.out.iter().rev() {
            if line.is_empty() {
                existing += 1;
            } else {
                break;
            }
        }
        for _ in existing..desired {
            self.out.push(String::new());
        }
    }

    fn skip_to_semicolon(&mut self) {
        let mut paren = 0i32;
        let mut bracket = 0i32;
        while let Some(t) = self.peek() {
            match t.text.as_str() {
                "(" => paren += 1,
                ")" => paren -= 1,
                "[" => bracket += 1,
                "]" => bracket -= 1,
                ";" if paren == 0 && bracket == 0 => {
                    self.pos += 1;
                    return;
                }
                "}" if paren == 0 && bracket == 0 => return,
                _ => {}
            }
            self.pos += 1;
        }
    }

    fn skip_statement(&mut self) {
        let mut brace = 0i32;
        while let Some(t) = self.peek() {
            match t.text.as_str() {
                "{" => brace += 1,
                "}" => {
                    if brace == 0 {
                        return;
                    }
                    brace -= 1;
                    self.pos += 1;
                    if brace == 0 {
                        return;
                    }
                    continue;
                }
                ";" if brace == 0 => {
                    self.pos += 1;
                    return;
                }
                _ => {}
            }
            self.pos += 1;
        }
    }

    /// Consume tokens up to (but not including) a stop token at nesting depth 0.
    fn take_until_top_level(&mut self, stops: &[&str]) -> Vec<Token> {
        let start = self.pos;
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut brace = 0i32;
        while let Some(t) = self.peek() {
            let text = t.text.as_str();
            if text == "}" && brace == 0 {
                break;
            }
            if paren == 0 && bracket == 0 && brace == 0 && stops.contains(&text) {
                break;
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
            self.pos += 1;
        }
        self.tokens[start..self.pos].to_vec()
    }

    fn parse_paren_expr(&mut self) -> String {
        if !self.expect("(") {
            return "<missing>".to_string();
        }
        let toks = self.take_until_top_level(&[")"]);
        self.expect(")");
        if toks.is_empty() {
            return "true".to_string();
        }
        format_expr(&toks)
    }

    // -- top level ----------------------------------------------------------

    pub(crate) fn parse_unit(&mut self) {
        while !self.eof() {
            let before = self.pos;
            self.parse_top_level_item();
            if self.pos == before {
                self.pos += 1;
            }
        }
    }

    fn parse_top_level_item(&mut self) {
        if self.eof() {
            return;
        }
        self.preserve_blank_lines();
        if let Some(token) = self.peek() {
            self.source_indent = token.indent;
        }
        if self.peek().map(|t| t.kind) == Some(Kind::Preproc) {
            self.handle_preproc();
            return;
        }
        if self.peek_is_comment() {
            self.emit_comment();
            return;
        }
        let text = self.peek_text().to_string();
        match text.as_str() {
            ";" => {
                self.bump();
            }
            "typedef" => self.parse_typedef(),
            _ if is_struct_like(&text) && self.struct_def_ahead() => self.parse_struct_def(),
            _ if self.looks_like_top_level_statement() => self.parse_statement(),
            _ => match self.classify_top_level() {
                TopKind::FunctionDef => self.parse_function(),
                TopKind::Prototype => self.skip_to_semicolon(),
                TopKind::Declaration => self.parse_declaration(),
                TopKind::StructDef => self.parse_struct_def(),
                TopKind::Unknown => {
                    self.error("could not understand this top-level declaration");
                    self.skip_statement();
                }
            },
        }
    }

    /// Allow pasting naked statements and loop/conditional snippets, not just
    /// complete C translation units.
    fn looks_like_top_level_statement(&self) -> bool {
        let text = self.peek_text();
        if matches!(
            text,
            "if" | "else"
                | "while"
                | "for"
                | "do"
                | "switch"
                | "return"
                | "break"
                | "continue"
                | "goto"
                | "{"
        ) {
            return true;
        }
        if self.peek().map(|t| t.kind) != Some(Kind::Ident) {
            return false;
        }
        matches!(
            self.text_at(self.pos + 1),
            "=" | "+="
                | "-="
                | "*="
                | "/="
                | "%="
                | "&="
                | "|="
                | "^="
                | "<<="
                | ">>="
                | "("
                | "["
                | "."
                | "->"
                | "++"
                | "--"
        )
    }

    fn classify_top_level(&self) -> TopKind {
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut saw_eq = false;
        let mut first_close: Option<usize> = None;
        let mut first_open: Option<usize> = None;

        let mut j = self.pos;
        while j < self.tokens.len() {
            let t = self.tokens[j].text.as_str();
            match t {
                "(" => {
                    if paren == 0 && bracket == 0 && first_open.is_none() {
                        first_open = Some(j);
                    }
                    paren += 1;
                }
                ")" => {
                    paren -= 1;
                    if paren == 0 && bracket == 0 && first_close.is_none() {
                        first_close = Some(j);
                    }
                }
                "[" => bracket += 1,
                "]" => bracket -= 1,
                "=" if paren == 0 && bracket == 0 => saw_eq = true,
                "{" if paren == 0 && bracket == 0 => {
                    if saw_eq {
                        return TopKind::Declaration;
                    }
                    if is_struct_like(&self.tokens[self.pos].text) {
                        return TopKind::StructDef;
                    }
                    if first_close.is_some() {
                        return TopKind::FunctionDef;
                    }
                    return TopKind::Unknown;
                }
                ";" if paren == 0 && bracket == 0 => {
                    if saw_eq {
                        return TopKind::Declaration;
                    }
                    if let (Some(open), Some(close)) = (first_open, first_close)
                        && close + 1 == j
                        && open > self.pos
                        && self.tokens[open - 1].kind == Kind::Ident
                    {
                        return TopKind::Prototype;
                    }
                    return TopKind::Declaration;
                }
                _ => {}
            }
            j += 1;
        }
        TopKind::Unknown
    }

    fn struct_def_ahead(&self) -> bool {
        let mut i = self.pos + 1;
        if self.tokens.get(i).map(|t| t.kind) == Some(Kind::Ident) {
            i += 1;
        }
        self.text_at(i) == "{"
    }

    fn parse_typedef(&mut self) {
        let typedef_indent = self.peek().map(|t| t.indent).unwrap_or(self.source_indent);
        self.bump(); // typedef
        let toks = self.take_until_top_level(&[";"]);
        self.eat(";");
        let alias = toks
            .iter()
            .rev()
            .find(|t| t.kind == Kind::Ident && !is_type_word(&t.text) && !is_qualifier(&t.text))
            .or_else(|| toks.iter().rev().find(|t| t.kind == Kind::Ident))
            .map(|t| t.text.clone());

        if !toks.is_empty()
            && is_struct_like(&toks[0].text)
            && let Some(open) = toks.iter().position(|t| t.text == "{")
            && let Some(close) = matching_close(&toks, open, "{", "}")
        {
            let saved_tokens = std::mem::take(&mut self.tokens);
            let saved_pos = self.pos;
            self.tokens = toks[..=close].to_vec();
            self.pos = 0;
            self.parse_struct_def();
            self.tokens = saved_tokens;
            self.pos = saved_pos;
        }

        self.source_indent = typedef_indent;
        match alias {
            Some(name) => {
                self.blank();
                self.emit(format!("TYPE {name}"));
                self.blank();
            }
            None => self.error("could not find the name of this typedef"),
        }
    }

    fn handle_preproc(&mut self) {
        let token = match self.bump() {
            Some(t) => t,
            None => return,
        };
        self.source_indent = token.indent;
        let text = token.text.trim();
        let rest = text.strip_prefix('#').unwrap_or(text).trim_start();
        let keyword_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let keyword = &rest[..keyword_end];
        let args = rest[keyword_end..].trim();

        match keyword {
            "define" => {
                let name_end = args
                    .find(|c: char| c.is_whitespace() || c == '(')
                    .unwrap_or(args.len());
                let name = &args[..name_end];
                if name.is_empty() {
                    return;
                }
                if args[name_end..].starts_with('(') {
                    // Function-like macros are not expanded by this heuristic.
                    return;
                }
                let value = args[name_end..].trim();
                self.blank();
                if value.is_empty() {
                    self.emit(format!("CONSTANT {name}"));
                } else {
                    self.emit(format!("CONSTANT {name} TO {value}"));
                }
                self.blank();
            }
            "ifdef" | "ifndef" => {
                let directive = keyword.to_uppercase();
                if args.is_empty() {
                    self.emit(format!("#{directive}"));
                } else {
                    self.emit(format!("#{directive} {args}"));
                }
            }
            "if" => self.emit(format!("#IF {}", uppercase_defined(args))),
            "elif" => self.emit(format!("#ELIF {}", uppercase_defined(args))),
            "else" => self.emit("#ELSE"),
            "endif" => self.emit("#ENDIF"),
            _ => {
                // Other directives (#include, #pragma, ...) are intentionally ignored.
            }
        }
    }

    fn parse_function(&mut self) {
        let function_indent = self.peek().map(|t| t.indent).unwrap_or(self.source_indent);
        let open = match self.find_first_top_level_paren() {
            Some(i) if i > self.pos => i,
            _ => {
                self.error("malformed function definition");
                self.skip_statement();
                return;
            }
        };
        let name_idx = (self.pos..open)
            .rev()
            .find(|&i| self.tokens[i].kind == Kind::Ident && !is_type_word(&self.tokens[i].text))
            .unwrap_or(open - 1);
        let name = self.tokens[name_idx].text.clone();
        let return_type = format_type(&self.tokens[self.pos..name_idx]);
        let close = match matching_close(&self.tokens, open, "(", ")") {
            Some(c) => c,
            None => {
                self.error("unbalanced parentheses in function signature");
                self.skip_statement();
                return;
            }
        };
        let params = format_params(&self.tokens[open + 1..close]);
        self.pos = close + 1;

        let mut header = format!("FUNCTION {name}({params})");
        if !return_type.is_empty() && return_type != "void" && name != "main" {
            header.push_str(&format!(" RETURNS {return_type}"));
        }

        if self.check(";") {
            self.bump();
            return;
        }
        if !self.check("{") {
            self.error("expected '{' to start the function body");
            return;
        }

        self.blank();
        self.source_indent = function_indent;
        self.emit(header);
        self.indent += 1;
        self.parse_body();
        self.indent -= 1;
        self.source_indent = function_indent;
        self.emit("END FUNCTION");
        self.blank();
    }

    fn find_first_top_level_paren(&self) -> Option<usize> {
        let mut paren = 0i32;
        let mut bracket = 0i32;
        for i in self.pos..self.tokens.len() {
            match self.tokens[i].text.as_str() {
                "(" => {
                    if paren == 0 && bracket == 0 {
                        return Some(i);
                    }
                    paren += 1;
                }
                ")" => paren -= 1,
                "[" => bracket += 1,
                "]" => bracket -= 1,
                _ => {}
            }
        }
        None
    }

    fn parse_body(&mut self) {
        if self.eat("{") {
            while !self.eof() && !self.check("}") {
                let before = self.pos;
                self.parse_statement();
                if self.pos == before {
                    self.pos += 1;
                }
            }
            self.expect("}");
        } else {
            self.emit_comments();
            self.parse_statement();
        }
    }

    // -- declarations -------------------------------------------------------

    fn looks_like_declaration(&self) -> bool {
        let t = match self.peek() {
            Some(t) => t,
            None => return false,
        };
        if t.kind != Kind::Ident {
            return false;
        }
        if is_qualifier(&t.text) || is_type_word(&t.text) || is_struct_like(&t.text) {
            return true;
        }
        let next = self.text_at(self.pos + 1);
        if next.is_empty() {
            return false;
        }
        if self
            .tokens
            .get(self.pos + 1)
            .map(|t| t.kind == Kind::Ident)
            .unwrap_or(false)
        {
            return true;
        }
        if next == "*"
            && self
                .tokens
                .get(self.pos + 2)
                .map(|t| t.kind == Kind::Ident)
                .unwrap_or(false)
        {
            return true;
        }
        false
    }

    fn parse_declaration(&mut self) {
        let toks = self.take_until_top_level(&[";"]);
        self.eat(";");
        if toks.is_empty() {
            return;
        }
        self.emit_declaration_tokens(&toks);
    }

    fn emit_declaration_tokens(&mut self, toks: &[Token]) {
        let start = declarator_start(toks);
        if start >= toks.len() {
            return;
        }
        let type_name = format_type(&toks[..start]);
        let declarators = format_declarators(&toks[start..]);
        if declarators.is_empty() {
            self.error("could not parse this declaration");
            return;
        }

        let has_initializer = declarators.iter().any(|d| d.contains(" TO "));
        if !has_initializer {
            self.emit(format!(
                "INIT {}",
                join_type_declaration(&type_name, &declarators.join(", "))
            ));
            return;
        }

        for declarator in declarators {
            match declarator.split_once(" TO ") {
                Some((name, value)) => self.emit(format!(
                    "INIT {} TO {value}",
                    join_type_declaration(&type_name, name)
                )),
                None => self.emit(format!(
                    "INIT {}",
                    join_type_declaration(&type_name, &declarator)
                )),
            }
        }
    }

    fn parse_struct_def(&mut self) {
        let struct_indent = self.peek().map(|t| t.indent).unwrap_or(self.source_indent);
        let kind = self.peek_text().to_string();
        self.bump();
        let tag = if self.peek().map(|t| t.kind) == Some(Kind::Ident) {
            let tag = self.peek_text().to_string();
            self.bump();
            tag
        } else {
            String::new()
        };
        let label = match kind.as_str() {
            "struct" => "STRUCT",
            "union" => "UNION",
            _ => "ENUM",
        };

        if !self.check("{") {
            self.skip_to_semicolon();
            return;
        }

        self.blank();
        self.source_indent = struct_indent;
        if tag.is_empty() {
            self.emit(label.to_string());
        } else {
            self.emit(format!("{label} {tag}"));
        }
        self.bump(); // {
        self.indent += 1;
        if kind == "enum" {
            self.parse_enum_body();
        } else {
            self.parse_struct_body();
        }
        self.indent -= 1;
        self.expect("}");
        self.source_indent = struct_indent;
        self.emit(format!("END {label}"));

        let rest = self.take_until_top_level(&[";"]);
        self.eat(";");
        if !rest.is_empty() {
            self.source_indent = rest[0].indent;
            let type_name = if tag.is_empty() {
                kind.clone()
            } else {
                format!("{kind} {tag}")
            };
            let declarators = format_declarators(&rest);
            for declarator in declarators {
                self.emit(format!(
                    "INIT {}",
                    join_type_declaration(&type_name, &declarator)
                ));
            }
        }
        self.blank();
    }

    fn parse_struct_body(&mut self) {
        while !self.eof() && !self.check("}") {
            let before = self.pos;
            self.preserve_blank_lines();
            if self.eat(";") {
                continue;
            }
            if is_struct_like(self.peek_text()) && self.struct_def_ahead() {
                self.parse_struct_def();
                continue;
            }
            let toks = self.take_until_top_level(&[";"]);
            self.eat(";");
            if !toks.is_empty() {
                self.source_indent = toks[0].indent;
                self.emit_declaration_tokens(&toks);
            }
            if self.pos == before {
                self.pos += 1;
            }
        }
    }

    fn parse_enum_body(&mut self) {
        while !self.eof() && !self.check("}") {
            let before = self.pos;
            self.preserve_blank_lines();
            let toks = self.take_until_top_level(&[",", "}"]);
            if !toks.is_empty() {
                self.source_indent = toks[0].indent;
                if let Some(eq) = find_top_level(&toks, "=") {
                    let name = format_expr(&toks[..eq]);
                    let value = format_expr(&toks[eq + 1..]);
                    self.emit(format!("{name} TO {value}"));
                } else {
                    self.emit(format_expr(&toks));
                }
            }
            if self.eat(",") {
                continue;
            }
            if self.pos == before {
                self.pos += 1;
            }
        }
    }

    // -- statements ---------------------------------------------------------

    fn parse_statement(&mut self) {
        if self.eof() {
            return;
        }
        self.preserve_blank_lines();
        if let Some(token) = self.peek() {
            self.source_indent = token.indent;
        }
        if self.peek().map(|t| t.kind) == Some(Kind::Preproc) {
            self.handle_preproc();
            return;
        }
        if self.peek_is_comment() {
            self.emit_comment();
            return;
        }
        let text = self.peek_text().to_string();
        match text.as_str() {
            "{" => {
                self.bump();
                while !self.eof() && !self.check("}") {
                    let before = self.pos;
                    self.parse_statement();
                    if self.pos == before {
                        self.pos += 1;
                    }
                }
                self.expect("}");
            }
            ";" => {
                self.bump();
            }
            "if" => self.parse_if(),
            "else" => self.parse_else(),
            "while" => self.parse_while(),
            "do" => self.parse_do(),
            "for" => self.parse_for(),
            "switch" => self.parse_switch(),
            "return" => self.parse_return(),
            "break" => self.parse_break(),
            "continue" => {
                self.bump();
                self.eat(";");
                self.emit("CONTINUE");
            }
            "goto" => {
                self.bump();
                let label = if self.peek().map(|t| t.kind) == Some(Kind::Ident) {
                    self.peek_text().to_string()
                } else {
                    "?".to_string()
                };
                let _ = self.take_until_top_level(&[";"]);
                self.eat(";");
                self.emit(format!("GOTO {label}"));
            }
            "typedef" => self.parse_typedef(),
            "case" | "default" => {
                self.error("'case'/'default' outside of a switch statement");
                self.skip_statement();
            }
            _ => {
                if is_struct_like(&text) && self.struct_def_ahead() {
                    self.parse_struct_def();
                } else if self.looks_like_declaration() {
                    self.parse_declaration();
                } else if self.peek().map(|t| t.kind) == Some(Kind::Ident)
                    && self.text_at(self.pos + 1) == ":"
                {
                    let label = self.peek_text().to_string();
                    self.bump();
                    self.bump();
                    self.emit(format!("LABEL {label}:"));
                } else {
                    let toks = self.take_until_top_level(&[";"]);
                    if !self.eat(";") && !self.check("}") && !self.eof() {
                        self.error("expected ';' at the end of the statement");
                    }
                    if let Some(line) = format_inline(&toks, true, &self.rules) {
                        self.emit(line);
                    }
                }
            }
        }
    }

    fn parse_if(&mut self) {
        let if_indent = self.peek().map(|t| t.indent).unwrap_or(self.source_indent);
        self.bump(); // if
        let cond = self.parse_paren_expr();
        self.source_indent = if_indent;
        self.emit(format!("IF ({cond})"));
        self.indent += 1;
        self.parse_body();
        self.indent -= 1;
        self.parse_if_tail(if_indent);
    }

    fn parse_else(&mut self) {
        let else_indent = self.peek().map(|t| t.indent).unwrap_or(self.source_indent);
        self.bump(); // else
        self.source_indent = else_indent;
        if self.check("if") {
            self.bump();
            let cond = self.parse_paren_expr();
            self.source_indent = else_indent;
            self.emit(format!("ELSE IF ({cond})"));
            self.indent += 1;
            self.parse_body();
            self.indent -= 1;
            self.parse_if_tail(else_indent);
        } else {
            self.emit("ELSE");
            self.indent += 1;
            self.parse_body();
            self.indent -= 1;
            self.source_indent = else_indent;
            self.emit("ENDIF");
        }
    }

    fn parse_if_tail(&mut self, if_indent: usize) {
        let saved_pos = self.pos;
        let saved_len = self.out.len();
        self.emit_comments();

        if self.eat("else") {
            self.source_indent = if_indent;
            if self.check("if") {
                self.bump();
                let cond = self.parse_paren_expr();
                self.source_indent = if_indent;
                self.emit(format!("ELSE IF ({cond})"));
                self.indent += 1;
                self.parse_body();
                self.indent -= 1;
                self.parse_if_tail(if_indent);
            } else {
                self.emit("ELSE");
                self.indent += 1;
                self.parse_body();
                self.indent -= 1;
                self.source_indent = if_indent;
                self.emit("ENDIF");
            }
        } else {
            self.pos = saved_pos;
            self.out.truncate(saved_len);
            self.source_indent = if_indent;
            self.emit("ENDIF");
        }
    }

    fn parse_while(&mut self) {
        let while_indent = self.peek().map(|t| t.indent).unwrap_or(self.source_indent);
        self.bump(); // while
        let cond = self.parse_paren_expr();
        self.source_indent = while_indent;
        self.emit(format!("WHILE ({cond})"));
        self.break_stack.push(BreakCtx::Loop);
        self.indent += 1;
        self.parse_body();
        self.indent -= 1;
        self.break_stack.pop();
        self.source_indent = while_indent;
        self.emit("ENDWHILE");
    }

    fn parse_do(&mut self) {
        let do_indent = self.peek().map(|t| t.indent).unwrap_or(self.source_indent);
        self.bump(); // do
        self.source_indent = do_indent;
        self.emit("DO");
        self.break_stack.push(BreakCtx::Loop);
        self.indent += 1;
        self.parse_body();
        self.indent -= 1;
        self.break_stack.pop();
        self.emit_comments();
        if self.eat("while") {
            let cond = self.parse_paren_expr();
            self.eat(";");
            self.source_indent = do_indent;
            self.emit(format!("WHILE ({cond})"));
        } else {
            self.error("expected 'while' after the body of a do-loop");
        }
    }

    fn parse_for(&mut self) {
        let for_indent = self.peek().map(|t| t.indent).unwrap_or(self.source_indent);
        self.bump(); // for
        if !self.expect("(") {
            self.skip_statement();
            return;
        }
        let init = self.take_until_top_level(&[";"]);
        self.expect(";");
        let cond = self.take_until_top_level(&[";"]);
        self.expect(";");
        let update = self.take_until_top_level(&[")"]);
        self.expect(")");

        let init_s = format_for_part(&init, &self.rules);
        let cond_s = if cond.is_empty() {
            "true".to_string()
        } else {
            format_expr(&cond)
        };
        let update_s = format_for_part(&update, &self.rules);
        let header = if init.is_empty() && cond.is_empty() && update.is_empty() {
            "FOREVER".to_string()
        } else {
            format!("FOR ({init_s}; {cond_s}; {update_s})")
        };
        self.source_indent = for_indent;
        self.emit(header);

        self.break_stack.push(BreakCtx::Loop);
        self.indent += 1;
        self.parse_body();
        self.indent -= 1;
        self.break_stack.pop();
        self.source_indent = for_indent;
        self.emit("ENDFOR");
    }

    fn parse_switch(&mut self) {
        let switch_indent = self.peek().map(|t| t.indent).unwrap_or(self.source_indent);
        self.bump(); // switch
        let cond = self.parse_paren_expr();
        self.source_indent = switch_indent;
        self.emit(format!("SWITCH ({cond})"));
        if !self.expect("{") {
            return;
        }
        self.force_structural = true;
        self.break_stack.push(BreakCtx::Switch);
        while !self.eof() && !self.check("}") {
            let before = self.pos;
            self.preserve_blank_lines();
            if self.peek_is_comment() {
                self.emit_comment();
                continue;
            }
            if self.check("case") {
                let case_indent = self.peek().map(|t| t.indent).unwrap_or(switch_indent);
                self.bump();
                let value = self.take_until_top_level(&[":"]);
                self.expect(":");
                self.source_indent = case_indent;
                self.emit(format!("CASE {}:", format_expr(&value)));
                continue;
            }
            if self.check("default") {
                let default_indent = self.peek().map(|t| t.indent).unwrap_or(switch_indent);
                self.bump();
                self.expect(":");
                self.source_indent = default_indent;
                self.emit("DEFAULT:");
                continue;
            }
            self.indent += 1;
            self.parse_statement();
            self.indent -= 1;
            if self.pos == before {
                self.pos += 1;
            }
        }
        self.break_stack.pop();
        self.force_structural = false;
        self.expect("}");
    }

    fn parse_return(&mut self) {
        self.bump(); // return
        let toks = self.take_until_top_level(&[";"]);
        self.eat(";");
        if toks.is_empty() {
            self.emit("RETURN");
        } else {
            self.emit(format!("RETURN {}", format_expr(&toks)));
        }
    }

    fn parse_break(&mut self) {
        self.bump(); // break
        self.eat(";");
        self.emit("BREAK");
    }
}
