# cpseudo

`cpseudo` is a native desktop tool for turning C source code into readable,
language-neutral pseudocode and for searching large C codebases by logical
code blocks.

It is designed for documentation, code review, legacy-code analysis, and
creating Excel-friendly reports. It runs entirely on your machine and does not
use an AI model, cloud service, or network API.

Repository: <https://github.com/andreiraphel/web-ui-tool>

## Features

- **C to pseudocode conversion**
  - Function bodies (signatures and `FUNCTION`/`END FUNCTION` wrappers are omitted)
  - Declarations, assignments, expressions, calls
  - `if` / `else if` / `else`
  - `for`, `while`, `do`, `switch` / `case`
  - Structs, unions, enums, and typedefs
  - Preprocessor conditionals (`#if`, `#ifdef`, `#ifndef`, `#else`, `#endif`)
  - Comments and blank lines
  - Pasted snippets, not just complete C files
- **Code search**
  - Fast folder/file search using ripgrep
  - Expands matches into complete `#if ... #endif`, `if/else`, loop, and switch blocks
  - Preferred block view or classic line-by-line results
  - Excel-ready clipboard output
- **Custom pseudocode dialect**
  - Rename generated keywords and phrases
  - Map C function calls such as `printf` to `OUTPUT`
  - Apply plain-text replacements outside string literals
- **CLI and GUI**
  - Interactive native interface
  - Command-line conversion for scripts and automation

## No AI, no cloud

`cpseudo` does not use an LLM, machine learning, or any remote service.

Conversion is performed by a hand-written C tokenizer, a recursive-descent-style
parser, and a formatter with explicit rules. Search uses ripgrep plus custom
block-detection logic. The same input always produces the same output, and
source code never leaves the machine.

## Installation

### Windows release

1. Open the repository's **Releases** page.
2. Download `cpseudo.exe`, or `cpseudo-windows.zip` if you prefer an archive.
3. If you downloaded the zip, extract it.
4. Run `cpseudo.exe`.

The executable is self-contained: ripgrep is embedded, so there is nothing else
to install or keep next to it.

### Build from source

Prerequisites:

- Rust stable with support for the 2024 edition
- ripgrep (`rg`) for the search tab, unless you embed it at build time (below)

```bash
git clone https://github.com/andreiraphel/web-ui-tool.git
cd web-ui-tool
cargo build --release
```

The binary is written to:

- Linux/macOS: `target/release/cpseudo`
- Windows: `target\release\cpseudo.exe`

To build a single-file binary with ripgrep embedded, put a ripgrep executable at
`vendor/rg.exe` (Windows) or `vendor/rg` (Linux/macOS) before building, or point
`CPSEUDO_BUNDLE_RG` at it:

```bash
CPSEUDO_BUNDLE_RG=/path/to/rg cargo build --release
```

Without that, the binary uses the ripgrep it finds at runtime (see below).

You can also run it directly with:

```bash
cargo run --release
```

### Installing ripgrep

Use your platform's package manager, for example:

```bash
# Debian / Ubuntu
sudo apt install ripgrep

# macOS
brew install ripgrep

# Windows
winget install BurntSushi.ripgrep.MSVC
```

Official releases embed ripgrep and extract it to a per-user cache directory the
first time search runs. Source builds that did not embed ripgrep use a system
copy.

The application looks for ripgrep in this order:

1. The path in the `CPSEUDO_RG` environment variable
2. The ripgrep embedded in the executable (official releases)
3. `rg` / `rg.exe` next to the application executable
4. `rg` on the system `PATH`

## Usage

### Running

Run the executable directly from the folder that contains it.

PowerShell:

```powershell
.\cpseudo.exe
.\cpseudo.exe .\src\main.c
```

Bash (Git Bash, WSL, Linux, macOS):

```bash
./cpseudo.exe          # Windows build
./cpseudo              # Linux/macOS build
./cpseudo src/main.c   # convert a file
```

The GUI needs a desktop session; on WSL use WSLg or an X server.

### Graphical interface

Launch `cpseudo` without arguments:

```bash
cpseudo
```

The application has two tabs.

#### C to Pseudocode

- Paste or load C code in the editor.
- Press `Ctrl+Enter` to convert.
- Press `Ctrl+Shift+C` to copy the pseudocode with bold keywords.
- Use **Copy** for plain text or **Copy bold** for Word/Docs-friendly output.

#### Grep / Blocks

- Choose a folder or a single file.
- Enter a regex or fixed-text pattern.
- Optionally limit files with a glob such as `*.c, *.h`.
- Enable **Expand #if ... #endif** and **Expand if / loop / switch blocks**
  to return complete logical blocks instead of isolated lines.
- **Copy preferred (Excel)** copies one row per block, with the complete block
  in a single Excel cell and indentation preserved.
- **Copy classic (Excel)** copies AgentRansack-style line-by-line rows.

### Command line

```bash
# Launch the GUI
cpseudo

# Convert a C file and print the pseudocode
cpseudo path/to/file.c

# Read C source from standard input
cat file.c | cpseudo -

# Load a custom rules file
cpseudo --rules my-rules.txt path/to/file.c

# Show help or version
cpseudo --help
cpseudo --version
```

## Custom pseudocode rules

Rules are line-oriented:

```text
# Rename a generated keyword or phrase
term FUNCTION => PROCEDURE
term PRINT => OUTPUT
term TO => :=

# Rewrite a C function call used as a statement
call printf => OUTPUT {args}
call scanf => INPUT {args}
call log_debug => LOG {1}

# Plain replacement outside string literals
text -> => .
```

`{args}` expands to all call arguments. `{1}`, `{2}`, and so on expand to
individual arguments.

Rules are loaded from, in order of precedence:

1. The file passed with `--rules`
2. The path in the `CPSEUDO_RULES` environment variable
3. `$XDG_CONFIG_HOME/cpseudo/equivalents.txt`, or
   `~/.config/cpseudo/equivalents.txt` when `XDG_CONFIG_HOME` is unset
4. Built-in defaults

Rules can also be edited and saved from the GUI.

## How it works

### Conversion pipeline

1. Tabs are expanded and the source is tokenized.
2. The parser recognizes C declarations, functions, control flow, and
   preprocessor directives.
3. The formatter emits readable pseudocode with indentation.
4. User-defined rules rewrite keywords, phrases, and calls.

The parser is intentionally heuristic rather than a complete C compiler
frontend. It focuses on the constructs commonly found in embedded and
system-level C code.

### Search pipeline

1. ripgrep finds matching lines and returns JSON output.
2. `cpseudo` reads the source file and masks comments and string literals.
3. Preprocessor and control-flow blocks are detected.
4. Each match is expanded to the tightest relevant block.
5. Overlapping blocks are merged and displayed or copied as a table.

## Technology

- **Rust 2024** - implementation language
- **eframe / egui** - native cross-platform GUI
- **arboard** - rich-text and plain-text clipboard access
- **serde_json** - parsing ripgrep JSON output
- **ripgrep** - fast code search backend
- **GitHub Actions** - automated Windows release builds

The converter uses a custom lexer, parser, formatter, and rule engine. No
third-party parsing framework or AI service is involved.

## Why use cpseudo?

- **Private and offline** - source code never leaves your computer.
- **Deterministic** - no model variation or hallucination.
- **Fast** - native Rust performance and ripgrep-based search.
- **No operating cost** - no API keys, subscriptions, or token usage.
- **Customizable** - adapt the pseudocode vocabulary to your team or standard.
- **Practical search results** - get complete code blocks instead of scattered
  matching lines.
- **Report-friendly** - paste structured results directly into Excel.

## Limitations

- Function signatures are not emitted; only function bodies are converted.
- The converter is not a full C compiler and does not perform semantic analysis.
- Preprocessor handling is heuristic; it is not a complete C preprocessor.
- Unusual or highly complex C constructs may need manual cleanup.
- The search block detector focuses on common brace-based control flow.
- ripgrep uses Rust regex syntax, which does not support backreferences or
  lookaround.
- Automated release builds currently target Windows only.

## Development

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Pushing a tag matching `v*` triggers the Windows release workflow, which builds
the application, bundles ripgrep, and attaches `cpseudo-windows.zip` to the
GitHub release.
