# AGENTS.md — .github

`workflows/windows.yml` is the release pipeline:

- Triggers: push tag `v*`, or manual `workflow_dispatch`.
- Runs on `windows-latest`.
- Downloads ripgrep 14.1.1 into `vendor/rg.exe` **before** the build so
  `build.rs` embeds it, then `cargo build --release`.
- Uploads both `target/release/cpseudo.exe` and `cpseudo-windows.zip` as build
  artifacts; for tags, attaches both files to the GitHub release so users can
  download the bare exe or a zip.
- There is no cross-platform CI and no test/lint job. Run `cargo test`,
  `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings` locally
  before tagging.
- If you change the bundled ripgrep version, update the `$version` variable and
  download URL in the workflow.
