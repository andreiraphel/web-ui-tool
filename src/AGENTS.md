# AGENTS.md — src

`main.rs` declares `mod converter; mod search;` and is the only binary root.

## main.rs (entry point + GUI + CLI)

Line map (approximate; confirm with a targeted read before editing):

- L1–26: imports, `SAMPLE` embed, bold-font candidate paths.
- L28–85: `KEYWORDS` — words rendered bold in the GUI and HTML export. Must
  mirror the emitted dialect; keep in sync with `src/converter`.
- L87–128: `main` — `-h/--help`, `-V/--version`, `--rules FILE`, one input path
  or `-`; no input launches the GUI.
- L130–199: help text, rules path resolution, `read_source`, `run_cli`.
- L201–257: native window options, bold-font install, dark style.
- L263–398: rich-text layout job + `bold_html` for the clipboard.
- L400–1008: `Tab`/`GrepView`, `GrepState`, `App` (state, `convert`, clipboard
  helpers, background search start/poll, grep UI, rules window).
- L1010–1236: `eframe::App::ui` — toolbar, status bar, Convert tab, Grep tab.
- L1238–1258: tests for keyword bolding and HTML escaping.

Notes:
- GUI live-converts on every edit when `live` is on.
- `copy_formatted`/`copy_rich` and `append_rich_text`/`append_html_text` are
  near-duplicates; consolidate if you touch them.
- Convert output is displayed read-only via a borrowed `&str`.

## search.rs

- `rg_program()` resolves `CPSEUDO_RG` → embedded ripgrep (extracted to a cache
  directory on first use) → `rg(.exe)` next to the exe → `PATH`.
- Public model: `SearchOptions`, `SearchResult`, `ClassicRow`, `BlockRow`.
- `run()` spawns `rg --json`, parses match records, re-reads each matched file,
  masks comments/strings (`MaskState`), detects `#if…#endif`
  (`PreprocessorBlocks`) and brace-based control flow (`control_blocks`), picks
  the tightest enclosing block, merges overlapping ranges, and fills both
  classic rows and block rows.
- Export helpers: `blocks_tsv`/`classic_tsv` (plain) and
  `blocks_html`/`classic_html` (Excel-friendly paste).
- `format_time` is a hand-rolled **UTC** formatter; `human_size`/`file_type`
  fill metadata columns.
- Line indices inside `run` are 0-based; exported rows use 1-based numbers.
- Tests: unit tests in `mod tests`; two top-level tests at the bottom spawn real
  `rg` and need a temp dir + ripgrep.

Guidelines:
- `run()` only reads files; it is invoked on a worker thread, keep it free of UI.
- When changing block detection, update `mod tests` and the two rg-backed tests.
