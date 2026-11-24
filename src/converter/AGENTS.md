# AGENTS.md — converter

C → pseudocode pipeline. Public entry point: `convert_with_rules(source, &Rules)
-> Conversion` in `mod.rs`. `convert()` is a test-only convenience wrapper.

Pipeline (`mod.rs::convert_with_rules`):

1. `expand_tabs` (tabs → 4 spaces)
2. `lex` → `Vec<Token>` (kinds: `Ident/Number/Str/Char/Punct/Preproc/Comment`)
3. `parser::Parser::new(tokens, lex_errors, rules.clone())` → `parse_unit()` → `finish()`
4. `rules.apply(&code)` — `term`/`text` rewrites, string-literal aware
5. lexer + parser errors sorted by line and returned in `Conversion.errors`

## mod.rs

Lexer and token model. Keyword/type tables: `TYPE_WORDS`, `QUALIFIERS`,
`STORAGE_WORDS`, `ASSIGN_OPS`, `BINARY_OPS`, `MULTI_OPS`. Add new C types or
keywords here first. Preprocessor directives are recognized only at line start;
line continuations are joined.

## parser.rs

Heuristic top-level classifier (`classify_top_level`) followed by per-construct
emitters. Emits: `IF/ELSE IF/ELSE/ENDIF`,
`WHILE/ENDWHILE`, `FOR/ENDFOR`, `FOREVER`, `DO … WHILE`, `SWITCH` with
`CASE:`/`DEFAULT:` (no closing keyword), `RETURN/BREAK/CONTINUE/GOTO/LABEL`,
`INIT`, `SET`, `CALL`, `STRUCT/UNION/ENUM … END`, `TYPE`, `CONSTANT`,
`#IF/#ELIF/#ELSE/#ENDIF`; comments are preserved. Function definitions
contribute only their body: no `FUNCTION …` header, parameter list, return
type, or `END FUNCTION` wrapper is emitted.

- `emit()` indents to `max(structural 4*indent, source_indent)`;
  `force_structural` is used inside `switch`.
- The parser is defensive: every loop checks that `pos` advanced to avoid
  infinite loops on malformed input.

## format.rs

- `format_expr` renders expressions with spacing rules; C casts detected via
  `cast_flags`.
- Helpers for types, declarators, and `for` parts.
- Function calls emit `CALL name(args)`; a leading cast is preserved. There
  are no built-in `printf`/`scanf`/`puts` conversions. User `call` rules
  override the mapping; templates support `{args}` and `{n}`.
- Multi-line source expressions keep line breaks using source token indentation
  (known alignment caveat when source indent ≠ output nesting).

## rules.rs

- Grammar: `term FROM => TO`, `call NAME => TEMPLATE`, `text FROM => TO`;
  `#` starts a comment; blank lines ignored.
- `Rules::from_text` returns `(Rules, Vec<String>)` with human-readable errors.
- `apply` skips string/char literals; rewrites run in file order.
- `DEFAULT_RULES` holds the commented default file shipped to users.

## tests.rs

- Uses a `check(source, expected_lines)`-style harness.
- Add/adjust a test for every behavior change.
- `bundled_example_converts_cleanly` asserts `examples/sample.c` converts with
  zero errors.
