use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct SearchOptions {
    pub root: String,
    pub pattern: String,
    pub glob: String,
    pub fixed_strings: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub hidden: bool,
    pub no_ignore: bool,
    pub context: usize,
    pub expand_preprocessor: bool,
    pub expand_control: bool,
    pub windows_paths: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            root: ".".to_string(),
            pattern: String::new(),
            glob: String::new(),
            fixed_strings: false,
            case_sensitive: false,
            whole_word: false,
            hidden: false,
            no_ignore: false,
            context: 2,
            expand_preprocessor: true,
            expand_control: true,
            windows_paths: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClassicRow {
    pub folder: String,
    pub file: String,
    pub size: String,
    pub file_type: String,
    pub modified: String,
    pub created: String,
    pub accessed: String,
    pub matches: usize,
    pub line: usize,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct BlockRow {
    pub path: String,
    pub lines: String,
    pub start: usize,
    pub end: usize,
    pub block: String,
}

#[derive(Clone, Debug, Default)]
pub struct SearchResult {
    pub classic: Vec<ClassicRow>,
    pub blocks: Vec<BlockRow>,
    pub files_with_matches: usize,
    pub match_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
struct MatchRow {
    path: PathBuf,
    line: usize,
    text: String,
}

fn rg_name() -> &'static str {
    if cfg!(windows) { "rg.exe" } else { "rg" }
}

fn rg_program() -> PathBuf {
    if let Some(path) = std::env::var_os("CPSEUDO_RG") {
        return PathBuf::from(path);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(rg_name());
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from(rg_name())
}

pub fn run(options: &SearchOptions) -> Result<SearchResult, String> {
    if options.pattern.trim().is_empty() {
        return Err("Enter a search pattern first".to_string());
    }

    let root = PathBuf::from(options.root.trim());
    if root.as_os_str().is_empty() {
        return Err("Enter a folder or file to search".to_string());
    }
    let root =
        fs::canonicalize(&root).map_err(|e| format!("Cannot open {}: {e}", root.display()))?;
    let base = if root.is_file() {
        root.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.clone())
    } else {
        root.clone()
    };

    let mut command = Command::new(rg_program());
    command
        .arg("--json")
        .arg("--line-number")
        .arg("--color")
        .arg("never")
        .arg("--no-messages");
    if !options.case_sensitive {
        command.arg("--ignore-case");
    }
    if options.fixed_strings {
        command.arg("--fixed-strings");
    }
    if options.whole_word {
        command.arg("--word-regexp");
    }
    if options.hidden {
        command.arg("--hidden");
    }
    if options.no_ignore {
        command.arg("--no-ignore");
    }
    for glob in options
        .glob
        .split(',')
        .map(str::trim)
        .filter(|glob| !glob.is_empty())
    {
        command.arg("--glob").arg(glob);
    }
    command.arg("--").arg(&options.pattern).arg(&root);

    let output = command.output().map_err(|e| {
        format!(
            "Could not run ripgrep (rg): {e}\n\
             Install ripgrep or place {} next to the app.",
            rg_name()
        )
    })?;

    if output.status.code().unwrap_or(2) > 1 {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "ripgrep failed".to_string()
        } else {
            stderr
        });
    }

    let mut matches = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("type").and_then(|v| v.as_str()) != Some("match") {
            continue;
        }
        let data = match value.get("data") {
            Some(data) => data,
            None => continue,
        };
        let path_text = data
            .get("path")
            .and_then(|p| p.get("text"))
            .and_then(|p| p.as_str())
            .unwrap_or("");
        let line_number = data
            .get("line_number")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as usize;
        let text = data
            .get("lines")
            .and_then(|l| l.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim_end_matches(['\n', '\r'])
            .to_string();
        if path_text.is_empty() || line_number == 0 {
            continue;
        }
        let mut path = PathBuf::from(path_text);
        if path.is_relative() {
            path = base.join(path);
        }
        matches.push(MatchRow {
            path,
            line: line_number,
            text,
        });
    }

    matches.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));

    let mut by_file: BTreeMap<PathBuf, Vec<MatchRow>> = BTreeMap::new();
    for m in matches {
        by_file.entry(m.path.clone()).or_default().push(m);
    }

    let mut result = SearchResult {
        files_with_matches: by_file.len(),
        match_count: by_file.values().map(Vec::len).sum(),
        ..SearchResult::default()
    };

    let mut warnings = Vec::new();

    for (path, file_matches) in &by_file {
        let metadata = fs::metadata(path).ok();
        let folder = path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let folder = with_trailing_separator(&folder, options.windows_paths);
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let size = metadata
            .as_ref()
            .map(|m| human_size(m.len()))
            .unwrap_or_default();
        let file_type = file_type(path);
        let modified = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .map(format_time)
            .unwrap_or_default();
        let created = metadata
            .as_ref()
            .and_then(|m| m.created().ok())
            .map(format_time)
            .unwrap_or_default();
        let accessed = metadata
            .as_ref()
            .and_then(|m| m.accessed().ok())
            .map(format_time)
            .unwrap_or_default();
        for m in file_matches {
            result.classic.push(ClassicRow {
                folder: folder.clone(),
                file: file.clone(),
                size: size.clone(),
                file_type: file_type.clone(),
                modified: modified.clone(),
                created: created.clone(),
                accessed: accessed.clone(),
                matches: file_matches.len(),
                line: m.line,
                text: m.text.clone(),
            });
        }
    }

    for (path, file_matches) in &by_file {
        let raw = match fs::read(path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(e) => {
                warnings.push(format!("Cannot read {}: {e}", path.display()));
                continue;
            }
        };
        let lines: Vec<String> = raw
            .lines()
            .map(|line| line.trim_end_matches('\r').to_string())
            .collect();
        if lines.is_empty() {
            continue;
        }
        let masked = mask_lines(&lines);
        let pp = PreprocessorBlocks::new(&masked);
        let controls = control_blocks(&masked);
        let mut ranges = Vec::new();
        for m in file_matches {
            if m.line == 0 || m.line > lines.len() {
                continue;
            }
            let idx = m.line - 1;
            let directive_line = masked[idx].trim_start().starts_with('#');
            let mut range = if options.expand_preprocessor {
                pp.expand(idx)
            } else {
                None
            };
            if range.is_none() && !directive_line && options.expand_control {
                range = enclosing_control(&controls, idx).map(|b| (b.start, b.end));
            }
            let structural = range.is_some() || directive_line;
            let (start, end) = range.unwrap_or((idx, idx));
            let (start, end) = if structural {
                (start, end)
            } else {
                (
                    start.saturating_sub(options.context),
                    (end + options.context).min(lines.len() - 1),
                )
            };
            ranges.push((start, end));
        }
        let merged = merge_ranges(ranges);
        let relative = relative_path(path, &base, options.windows_paths);
        for (start, end) in merged {
            let block = lines[start..=end].join("\n");
            result.blocks.push(BlockRow {
                path: relative.clone(),
                lines: if start == end {
                    format!("{}", start + 1)
                } else {
                    format!("{}, {}", start + 1, end + 1)
                },
                start: start + 1,
                end: end + 1,
                block,
            });
        }
    }

    result.warnings = warnings;
    Ok(result)
}

fn with_trailing_separator(path: &str, windows: bool) -> String {
    if path.is_empty() {
        return String::new();
    }
    let sep = if windows { '\\' } else { '/' };
    let trimmed = path.trim_end_matches(['/', '\\']);
    format!("{trimmed}{sep}")
}

fn relative_path(path: &Path, base: &Path, windows: bool) -> String {
    let rel = path.strip_prefix(base).unwrap_or(path);
    let sep = if windows { '\\' } else { '/' };
    let mut out = String::new();
    out.push(sep);
    for component in rel.components() {
        match component {
            Component::Normal(part) => {
                out.push_str(&part.to_string_lossy());
                out.push(sep);
            }
            Component::ParentDir => {
                out.push_str("..");
                out.push(sep);
            }
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    if out.len() > 1 {
        out.pop();
    }
    out
}

fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} bytes")
    }
}

fn file_type(path: &Path) -> String {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let label = match ext.as_str() {
        "c" => "C Source",
        "h" => "C/C++ Header",
        "cc" | "cpp" | "cxx" | "c++" => "C++ Source",
        "hh" | "hpp" | "hxx" | "h++" => "C++ Header",
        "rs" => "Rust Source",
        "py" => "Python Source",
        "js" | "mjs" | "cjs" => "JavaScript Source",
        "ts" | "tsx" => "TypeScript Source",
        "java" => "Java Source",
        "cs" => "C# Source",
        "go" => "Go Source",
        "txt" => "Text Document",
        "md" => "Markdown Document",
        "xml" => "XML Document",
        "json" => "JSON File",
        "html" | "htm" => "HTML Document",
        "css" => "Cascading Style Sheet",
        "sh" => "Shell Script",
        "" => "File",
        _ => return format!("{} File", ext.to_ascii_uppercase()),
    };
    label.to_string()
}

fn format_time(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn mask_lines(lines: &[String]) -> Vec<String> {
    let mut state = MaskState::default();
    lines
        .iter()
        .map(|line| state.mask(line))
        .collect::<Vec<_>>()
}

#[derive(Default)]
struct MaskState {
    block_comment: bool,
    quote: Option<char>,
}

impl MaskState {
    fn mask(&mut self, line: &str) -> String {
        let chars: Vec<char> = line.chars().collect();
        let mut out = String::with_capacity(line.len());
        let mut i = 0usize;
        while i < chars.len() {
            let c = chars[i];
            if self.block_comment {
                if c == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                    self.block_comment = false;
                    out.push_str("  ");
                    i += 2;
                } else {
                    out.push(' ');
                    i += 1;
                }
                continue;
            }
            if let Some(quote) = self.quote {
                if c == '\\' {
                    out.push(' ');
                    if i + 1 < chars.len() {
                        out.push(' ');
                        i += 2;
                    } else {
                        i += 1;
                    }
                } else if c == quote {
                    self.quote = None;
                    out.push(' ');
                    i += 1;
                } else {
                    out.push(' ');
                    i += 1;
                }
                continue;
            }
            if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
                break;
            }
            if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
                self.block_comment = true;
                out.push_str("  ");
                i += 2;
                continue;
            }
            if c == '"' || c == '\'' {
                self.quote = Some(c);
                out.push(' ');
                i += 1;
                continue;
            }
            out.push(c);
            i += 1;
        }
        out
    }
}

struct PreprocessorBlocks {
    open_to_end: HashMap<usize, usize>,
    line_to_open: HashMap<usize, usize>,
}

impl PreprocessorBlocks {
    fn new(masked: &[String]) -> Self {
        let mut open_to_end = HashMap::new();
        let mut line_to_open = HashMap::new();
        let mut stack: Vec<usize> = Vec::new();
        for (idx, line) in masked.iter().enumerate() {
            let Some(kind) = pp_kind(line) else {
                continue;
            };
            match kind {
                "if" | "ifdef" | "ifndef" => {
                    stack.push(idx);
                }
                "elif" | "else" => {
                    if let Some(open) = stack.last().copied() {
                        line_to_open.insert(idx, open);
                    }
                }
                "endif" => {
                    if let Some(open) = stack.pop() {
                        open_to_end.insert(open, idx);
                        line_to_open.insert(idx, open);
                    }
                }
                _ => {}
            }
        }
        Self {
            open_to_end,
            line_to_open,
        }
    }

    fn expand(&self, idx: usize) -> Option<(usize, usize)> {
        if let Some(&open) = self.line_to_open.get(&idx) {
            let end = self.open_to_end.get(&open)?;
            return Some((open, *end));
        }
        let end = self.open_to_end.get(&idx)?;
        Some((idx, *end))
    }
}

fn pp_kind(line: &str) -> Option<&'static str> {
    let text = line.trim_start();
    let rest = text.strip_prefix('#')?.trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_alphabetic() || c == '_'))
        .unwrap_or(rest.len());
    let word = &rest[..end];
    match word {
        "if" => Some("if"),
        "ifdef" => Some("ifdef"),
        "ifndef" => Some("ifndef"),
        "elif" => Some("elif"),
        "else" => Some("else"),
        "endif" => Some("endif"),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug)]
struct ControlBlock {
    start: usize,
    end: usize,
}

fn control_blocks(masked: &[String]) -> Vec<ControlBlock> {
    let mut open_to_close: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for (line_idx, line) in masked.iter().enumerate() {
        for (col, c) in line.chars().enumerate() {
            match c {
                '{' => stack.push((line_idx, col)),
                '}' => {
                    if let Some(open) = stack.pop() {
                        open_to_close.insert(open, (line_idx, col));
                    }
                }
                _ => {}
            }
        }
    }

    let mut blocks = Vec::new();
    for (idx, line) in masked.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        let Some(kind) = control_kind(trimmed) else {
            continue;
        };
        if kind == "else" {
            continue;
        }
        let Some((open_line, open_col)) = find_open_brace(masked, idx, line, 0) else {
            continue;
        };
        let Some(&(mut close_line, mut close_col)) = open_to_close.get(&(open_line, open_col))
        else {
            continue;
        };
        if kind == "if" {
            while let Some((next_line, next_col)) =
                else_chain(masked, &open_to_close, close_line, close_col)
            {
                if next_line <= close_line {
                    break;
                }
                close_line = next_line;
                close_col = next_col;
            }
        }
        blocks.push(ControlBlock {
            start: idx,
            end: close_line,
        });
    }
    blocks
}

fn control_kind(trimmed: &str) -> Option<&'static str> {
    let word_end = trimmed
        .find(|c: char| !(c.is_ascii_alphabetic() || c == '_'))
        .unwrap_or(trimmed.len());
    match &trimmed[..word_end] {
        "if" => Some("if"),
        "else" => Some("else"),
        "for" => Some("for"),
        "while" => Some("while"),
        "switch" => Some("switch"),
        "do" => Some("do"),
        _ => None,
    }
}

fn find_open_brace(
    masked: &[String],
    start: usize,
    first_line: &str,
    from_col: usize,
) -> Option<(usize, usize)> {
    let tail: String = first_line.chars().skip(from_col).collect();
    if let Some(byte) = tail.find('{') {
        let col = from_col + tail[..byte].chars().count();
        return Some((start, col));
    }
    for (idx, line) in masked.iter().enumerate().skip(start + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(byte) = line.find('{') {
            let col = line[..byte].chars().count();
            return Some((idx, col));
        }
        return None;
    }
    None
}

fn else_chain(
    masked: &[String],
    open_to_close: &HashMap<(usize, usize), (usize, usize)>,
    close_line: usize,
    close_col: usize,
) -> Option<(usize, usize)> {
    let mut line_idx = close_line;
    let mut col = close_col + 1;
    loop {
        let line = masked.get(line_idx)?;
        let tail: String = line.chars().skip(col).collect();
        let trimmed = tail.trim_start();
        if trimmed.is_empty() {
            line_idx += 1;
            col = 0;
            continue;
        }
        if !trimmed.starts_with("else") {
            return None;
        }
        let leading = tail.len() - trimmed.len();
        let open_from = col + leading + 4;
        let (open_line, open_col) = find_open_brace(masked, line_idx, line, open_from)?;
        let &(next_line, next_col) = open_to_close.get(&(open_line, open_col))?;
        return Some((next_line, next_col));
    }
}

fn enclosing_control(blocks: &[ControlBlock], idx: usize) -> Option<ControlBlock> {
    blocks
        .iter()
        .filter(|b| b.start <= idx && idx <= b.end)
        .max_by_key(|b| b.start)
        .copied()
}

fn merge_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    ranges.sort_by_key(|r| (r.0, r.1));
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut()
            && start <= last.1 + 1
        {
            last.1 = last.1.max(end);
        } else {
            merged.push((start, end));
        }
    }
    merged
}

pub fn blocks_tsv(result: &SearchResult, header: bool) -> String {
    let mut out = String::new();
    if header {
        out.push_str("File\tLines\tBlock\n");
    }
    for row in &result.blocks {
        out.push_str(&row.path);
        out.push('\t');
        out.push_str(&row.lines);
        out.push('\t');
        out.push_str(&row.block);
        out.push('\n');
    }
    out
}

pub fn classic_tsv(result: &SearchResult, header: bool) -> String {
    let mut out = String::new();
    if header {
        out.push_str(
            "Folder\tFile\tSize\tType\tModified\tCreated\tAccessed\tMatches\tLine\tText\n",
        );
    }
    for row in &result.classic {
        out.push_str(&row.folder);
        out.push('\t');
        out.push_str(&row.file);
        out.push('\t');
        out.push_str(&row.size);
        out.push('\t');
        out.push_str(&row.file_type);
        out.push('\t');
        out.push_str(&row.modified);
        out.push('\t');
        out.push_str(&row.created);
        out.push('\t');
        out.push_str(&row.accessed);
        out.push('\t');
        out.push_str(&row.matches.to_string());
        out.push('\t');
        out.push_str(&row.line.to_string());
        out.push('\t');
        out.push_str(&row.text);
        out.push('\n');
    }
    out
}

pub fn blocks_html(result: &SearchResult, header: bool) -> String {
    let mut rows = Vec::new();
    if header {
        rows.push(vec![
            "File".to_string(),
            "Lines".to_string(),
            "Block".to_string(),
        ]);
    }
    for row in &result.blocks {
        rows.push(vec![row.path.clone(), row.lines.clone(), row.block.clone()]);
    }
    html_table(&rows)
}

pub fn classic_html(result: &SearchResult, header: bool) -> String {
    let mut rows = Vec::new();
    if header {
        rows.push(
            [
                "Folder", "File", "Size", "Type", "Modified", "Created", "Accessed", "Matches",
                "Line", "Text",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );
    }
    for row in &result.classic {
        rows.push(vec![
            row.folder.clone(),
            row.file.clone(),
            row.size.clone(),
            row.file_type.clone(),
            row.modified.clone(),
            row.created.clone(),
            row.accessed.clone(),
            row.matches.to_string(),
            row.line.to_string(),
            row.text.clone(),
        ]);
    }
    html_table(&rows)
}

fn html_table(rows: &[Vec<String>]) -> String {
    let mut out = String::from("<html><body><table style=\"border-collapse:collapse\">");
    for row in rows {
        out.push_str("<tr>");
        for cell in row {
            out.push_str("<td style=\"white-space:pre-wrap;vertical-align:top\">");
            out.push_str(&html_escape(cell).replace('\n', "<br>"));
            out.push_str("</td>");
        }
        out.push_str("</tr>");
    }
    out.push_str("</table></body></html>");
    out
}

fn html_escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(|s| s.to_string()).collect()
    }

    #[test]
    fn preprocessor_open_and_close_expand_to_same_block() {
        let masked = mask_lines(&lines("#if defined(X)\nint a;\nint b;\n#endif\nint c;\n"));
        let pp = PreprocessorBlocks::new(&masked);
        assert_eq!(pp.expand(0), Some((0, 3)));
        assert_eq!(pp.expand(3), Some((0, 3)));
    }

    #[test]
    fn preprocessor_else_expands_to_full_conditional() {
        let masked = mask_lines(&lines("#if defined(X)\nint a;\n#else\nint b;\n#endif\n"));
        let pp = PreprocessorBlocks::new(&masked);
        assert_eq!(pp.expand(2), Some((0, 4)));
        assert_eq!(pp.expand(4), Some((0, 4)));
    }

    #[test]
    fn nested_preprocessor_blocks_are_matched() {
        let masked = mask_lines(&lines("#if A\n#if B\nint x;\n#endif\n#endif\n"));
        let pp = PreprocessorBlocks::new(&masked);
        assert_eq!(pp.expand(0), Some((0, 4)));
        assert_eq!(pp.expand(1), Some((1, 3)));
    }

    #[test]
    fn if_else_block_includes_else_branch() {
        let masked = mask_lines(&lines(
            "void f()\n{\n    if (a)\n    {\n        one();\n    }\n    else if (b)\n    {\n        two();\n    }\n    else\n    {\n        three();\n    }\n}\n",
        ));
        let blocks = control_blocks(&masked);
        let block = enclosing_control(&blocks, 4).unwrap();
        assert_eq!((block.start, block.end), (2, 13));
        assert_eq!(enclosing_control(&blocks, 12).unwrap().start, 2);
    }

    #[test]
    fn input_and_return_type_paths_are_windows_style() {
        let base = Path::new("/work/root");
        let path = Path::new("/work/root/a/b.c");
        assert_eq!(relative_path(path, base, true), "\\a\\b.c");
        assert_eq!(relative_path(path, base, false), "/a/b.c");
    }

    #[test]
    fn overlapping_ranges_merge() {
        assert_eq!(
            merge_ranges(vec![(2, 5), (4, 8), (10, 11)]),
            vec![(2, 8), (10, 11)]
        );
    }
}

#[test]
fn search_merges_preprocessor_and_if_else_blocks() {
    let dir = std::env::temp_dir().join(format!("cpseudo-grep-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("sample.c");
    fs::write(
        &file,
        "void f(int a)\n{\n#if defined(X)\n    if (a) {\n        one();\n    } else {\n        two();\n    }\n#endif\n}\n",
    )
    .unwrap();

    let options = SearchOptions {
        root: dir.display().to_string(),
        pattern: "defined|one|two".to_string(),
        windows_paths: true,
        ..SearchOptions::default()
    };
    let result = run(&options).unwrap();
    assert_eq!(result.match_count, 3);
    assert_eq!(result.files_with_matches, 1);
    assert_eq!(result.blocks.len(), 1, "{:?}", result.blocks);
    assert_eq!(result.blocks[0].path, "\\sample.c");
    assert_eq!(result.blocks[0].lines, "3, 9");
    assert!(result.blocks[0].block.contains("#if defined(X)"));
    assert!(result.blocks[0].block.contains("two();"));
    let _ = fs::remove_dir_all(&dir);
}
