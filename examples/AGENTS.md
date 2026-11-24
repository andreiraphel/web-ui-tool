# AGENTS.md — examples

`sample.c` is compiled into the binary via `include_str!("../examples/sample.c")`
in `src/main.rs` (`SAMPLE`) and `include_str!("../../examples/sample.c")` in
`src/converter/tests.rs`.

- It is the GUI's default editor content and the "Load example" target.
- It exercises the advertised feature set (functions, `for`, `do/while`,
  `switch`, preprocessor constant, struct, stdio calls) and must keep converting
  with zero errors (`bundled_example_converts_cleanly`).
- Do not add constructs the heuristic parser cannot handle without adding a test.
