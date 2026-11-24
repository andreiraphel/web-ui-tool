# AGENTS.md — cpseudo

Native Rust desktop app (eframe/egui) that turns C source into readable,
language-neutral pseudocode and searches C codebases by logical code blocks.
Offline and deterministic: no LLM, no network, no AI. Single binary crate named
`cpseudo`, edition 2024, **no lib target** — modules live under `src/`.

## Commands

- Build: `cargo build --release`
- Single-file build: place `vendor/rg.exe` (Windows) or `vendor/rg` (Unix), or set
  `CPSEUDO_BUNDLE_RG`, then build.
- GUI: `cargo run --release` (no args)
- CLI: `cargo run --release -- path/to/file.c` or `cargo run --release -- -` (stdin)
- Test: `cargo test`
- Gates that must stay green: `cargo fmt --check` and
  `cargo clippy --all-targets -- -D warnings`

`cargo test` includes two integration tests that spawn real `rg`, so ripgrep
must be installed.

## Layout — read the nested AGENTS.md before opening big files

- `src/main.rs` (~1260 lines): arg parsing, CLI, egui `App`, clipboard/HTML
  export, fonts/theme. See `src/AGENTS.md` for a line map.
- `src/search.rs` (~1040 lines): ripgrep orchestration, JSON parsing,
  comment/string masking, `#if`/control-flow block expansion, TSV/HTML export.
- `src/converter/`: C → pseudocode lexer, parser, formatter, rule engine. See
  `src/converter/AGENTS.md`.
- `examples/sample.c`: embedded with `include_str!`; must convert with zero
  errors. See `examples/AGENTS.md`.
- `build.rs`: optionally embeds ripgrep from `vendor/rg(.exe)` or
  `CPSEUDO_BUNDLE_RG` into the executable.
- `.github/workflows/windows.yml`: tag `v*` → single-file Windows release zip.
  See `.github/AGENTS.md`.

## Runtime behavior that is easy to break

- ripgrep resolution: `CPSEUDO_RG` → embedded (official build) → `rg(.exe)` next
  to the executable → `rg` on PATH.
- Rules resolution: `--rules` → `CPSEUDO_RULES` →
  `$XDG_CONFIG_HOME/cpseudo/equivalents.txt` (or `~/.config/...`) → built-in
  `DEFAULT_RULES`.
- Tabs are expanded to 4 spaces; structural pseudocode indent is 4 spaces/level.

## Conventions

- Keep changes surgical and consistent with existing style.
- The output dialect is line-oriented pseudocode. If you add or rename an
  emitted keyword, also update `KEYWORDS` in `src/main.rs` (bold highlighting)
  and the tests.
- Never add network calls, LLM/ML, or heavyweight dependencies; privacy and
  determinism are core product claims.
- Update `README.md` when user-visible behavior, CLI flags, or rule syntax change.

## Known rough edges (open items, fix deliberately)

- An explicit `--rules`/`CPSEUDO_RULES` path that fails to read silently falls
  back to defaults (`load_rules_text` in `src/main.rs`).
- `format_time` renders UTC, not local time.
- The CLI exits 0 even when conversion produced errors.
- `SWITCH` and `DO` blocks have no closing keyword, while `IF`/`WHILE`/`FOR` do.
  Changing this is a dialect change and breaks existing tests on purpose.
