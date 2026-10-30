mod converter;
mod search;

use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui;
use eframe::egui::{Color32, RichText};

use converter::{Conversion, ConvertError, DEFAULT_RULES, Rules, convert_with_rules};
use search::{SearchOptions, SearchResult};

const SAMPLE: &str = include_str!("../examples/sample.c");

const BOLD_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono-Bold.ttf",
    "/System/Library/Fonts/Supplemental/Courier New Bold.ttf",
    "C:\\Windows\\Fonts\\consolab.ttf",
    "C:\\Windows\\Fonts\\courbd.ttf",
];

const KEYWORDS: &[&str] = &[
    "INIT",
    "DECLARE",
    "SET",
    "TO",
    "CALL",
    "IF",
    "ELSE",
    "ENDIF",
    "WHILE",
    "ENDWHILE",
    "FOR",
    "ENDFOR",
    "FOREVER",
    "DO",
    "SWITCH",
    "CASE",
    "DEFAULT",
    "WHEN",
    "OTHERWISE",
    "BREAK",
    "CONTINUE",
    "RETURN",
    "GOTO",
    "LABEL",
    "STRUCT",
    "UNION",
    "ENUM",
    "END",
    "TYPE",
    "CONSTANT",
    "PRINT",
    "READ",
    "INCREMENT",
    "DECREMENT",
    "MULTIPLY",
    "DIVIDE",
    "BY",
    "AND",
    "OR",
    "NOT",
    "IS",
    "EQUAL",
    "GREATER",
    "THAN",
    "LESS",
    "FUNCTION",
    "RETURNS",
    "TRUE",
    "FALSE",
    "NULL",
    "THEN",
    "OF",
];

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut rules_file: Option<String> = None;
    let mut input: Option<String> = None;
    let mut index = 0usize;

    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "-V" | "--version" => {
                println!("cpseudo {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--rules" => {
                index += 1;
                if index >= args.len() {
                    eprintln!("error: --rules needs a file path");
                    std::process::exit(2);
                }
                rules_file = Some(args[index].clone());
            }
            other => {
                if input.is_some() {
                    eprintln!("error: more than one input file given");
                    std::process::exit(2);
                }
                input = Some(other.to_string());
            }
        }
        index += 1;
    }

    if let Some(path) = input {
        run_cli(&path, rules_file.as_deref());
        Ok(())
    } else {
        run_gui(rules_file.as_deref())
    }
}

fn print_help() {
    println!(
        "cpseudo - turn C code into language-neutral pseudocode\n\n\
         Usage:\n  \
         cpseudo                          launch the graphical interface\n  \
         cpseudo <file.c>                 print pseudocode for a file\n  \
         cpseudo -                        read C source from stdin\n  \
         cpseudo --rules <rules.txt> ...  load custom equivalents\n  \
         cpseudo --help                   show this help\n\n\
         GUI: paste C code on the left, or use Ctrl+V. \
         Ctrl+Enter converts, Ctrl+Shift+C copies the result.\n\
         Custom equivalents: term INIT => DECLARE, \
         call printf => OUTPUT {{args}}, term TO => :="
    );
}

fn rules_path(explicit: Option<&str>) -> Option<PathBuf> {
    explicit.map(PathBuf::from).or_else(default_rules_path)
}

fn default_rules_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CPSEUDO_RULES") {
        return Some(PathBuf::from(path));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("cpseudo").join("equivalents.txt"))
}

fn load_rules_text(path: &Option<PathBuf>) -> String {
    path.as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_else(|| DEFAULT_RULES.to_string())
}

fn read_source(path: &str) -> String {
    if path == "-" {
        let mut source = String::new();
        if std::io::stdin().read_to_string(&mut source).is_err() {
            eprintln!("error: could not read standard input");
            std::process::exit(1);
        }
        source
    } else {
        match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(e) => {
                eprintln!("error: could not read {path}: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn run_cli(path: &str, explicit_rules: Option<&str>) {
    let source = read_source(path);
    let rules_path = rules_path(explicit_rules);
    let rules_text = load_rules_text(&rules_path);
    let (rules, rule_errors) = Rules::from_text(&rules_text);
    for error in &rule_errors {
        eprintln!("rules warning: {error}");
    }

    let conversion = convert_with_rules(&source, &rules);
    print!("{}", conversion.code);
    for error in &conversion.errors {
        eprintln!("warning: {error}");
    }
}

fn run_gui(explicit_rules: Option<&str>) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([760.0, 480.0])
            .with_title("C to Pseudocode"),
        ..Default::default()
    };

    eframe::run_native(
        "C to Pseudocode",
        options,
        Box::new(|cc| {
            let bold_fonts = install_bold_font(&cc.egui_ctx);
            configure_style(&cc.egui_ctx);
            Ok(Box::new(App::new(explicit_rules, bold_fonts)))
        }),
    )
}

fn install_bold_font(ctx: &egui::Context) -> bool {
    for path in BOLD_FONT_PATHS {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "cpseudo_bold".to_owned(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts.families.insert(
            egui::FontFamily::Name("cpseudo-bold".into()),
            vec!["cpseudo_bold".to_owned()],
        );
        ctx.set_fonts(fonts);
        return true;
    }
    false
}

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = Color32::from_rgb(24, 26, 32);
    style.visuals.window_fill = Color32::from_rgb(30, 33, 40);
    style.visuals.extreme_bg_color = Color32::from_rgb(18, 20, 25);
    style.visuals.selection.bg_fill = Color32::from_rgb(60, 110, 190);
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(20.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.5));
    ctx.set_global_style(style);
}

fn is_keyword(word: &str) -> bool {
    KEYWORDS.contains(&word)
}

fn append_rich_text(
    job: &mut egui::text::LayoutJob,
    text: &str,
    normal: &egui::TextFormat,
    keyword: &egui::TextFormat,
) {
    let mut plain_start = 0usize;
    let mut word_start: Option<usize> = None;

    for (index, c) in text.char_indices() {
        if c.is_alphanumeric() || c == '_' {
            if word_start.is_none() {
                word_start = Some(index);
            }
        } else if let Some(start) = word_start.take() {
            job.append(&text[plain_start..start], 0.0, normal.clone());
            let word = &text[start..index];
            let format = if is_keyword(word) {
                keyword.clone()
            } else {
                normal.clone()
            };
            job.append(word, 0.0, format);
            plain_start = index;
        }
    }

    if let Some(start) = word_start {
        job.append(&text[plain_start..start], 0.0, normal.clone());
        let word = &text[start..];
        let format = if is_keyword(word) {
            keyword.clone()
        } else {
            normal.clone()
        };
        job.append(word, 0.0, format);
        plain_start = text.len();
    }

    if plain_start < text.len() {
        job.append(&text[plain_start..], 0.0, normal.clone());
    }
}

fn pseudocode_layout_job(
    ui: &egui::Ui,
    text: &str,
    wrap_width: f32,
    bold_fonts: bool,
) -> egui::text::LayoutJob {
    let normal = egui::TextFormat {
        font_id: egui::FontId::monospace(13.5),
        color: ui.visuals().text_color(),
        ..Default::default()
    };
    let family = if bold_fonts {
        egui::FontFamily::Name("cpseudo-bold".into())
    } else {
        egui::FontFamily::Monospace
    };
    let keyword = egui::TextFormat {
        font_id: egui::FontId::new(13.5, family),
        color: ui.visuals().strong_text_color(),
        ..Default::default()
    };

    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;
    job.break_on_newline = true;
    append_rich_text(&mut job, text, &normal, &keyword);
    job
}

fn escape_html(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

fn append_html_text(out: &mut String, text: &str) {
    let mut plain_start = 0usize;
    let mut word_start: Option<usize> = None;

    for (index, c) in text.char_indices() {
        if c.is_alphanumeric() || c == '_' {
            if word_start.is_none() {
                word_start = Some(index);
            }
        } else if let Some(start) = word_start.take() {
            escape_html(out, &text[plain_start..start]);
            let word = &text[start..index];
            if is_keyword(word) {
                out.push_str("<b>");
                escape_html(out, word);
                out.push_str("</b>");
            } else {
                escape_html(out, word);
            }
            plain_start = index;
        }
    }

    if let Some(start) = word_start {
        escape_html(out, &text[plain_start..start]);
        let word = &text[start..];
        if is_keyword(word) {
            out.push_str("<b>");
            escape_html(out, word);
            out.push_str("</b>");
        } else {
            escape_html(out, word);
        }
        plain_start = text.len();
    }

    if plain_start < text.len() {
        escape_html(out, &text[plain_start..]);
    }
}

fn bold_html(code: &str) -> String {
    let mut out = String::from(
        "<pre style=\"font-family:Consolas,'Courier New',monospace;font-size:11pt;white-space:pre;\">",
    );
    for line in code.lines() {
        append_html_text(&mut out, line);
        out.push('\n');
    }
    out.push_str("</pre>");
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Convert,
    Grep,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GrepView {
    Preferred,
    Classic,
}

struct GrepState {
    options: SearchOptions,
    result: Option<SearchResult>,
    status: String,
    running: bool,
    rx: Option<Receiver<Result<SearchResult, String>>>,
    include_header: bool,
    view: GrepView,
}

impl GrepState {
    fn new() -> Self {
        let root = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".to_string());
        Self {
            options: SearchOptions {
                root,
                ..SearchOptions::default()
            },
            result: None,
            status: "Choose a folder, enter a pattern, then press Search".to_string(),
            running: false,
            rx: None,
            include_header: false,
            view: GrepView::Preferred,
        }
    }
}

struct App {
    source: String,
    output: String,
    errors: Vec<ConvertError>,
    live: bool,
    status: String,
    rules: Rules,
    rules_text: String,
    rules_errors: Vec<String>,
    rules_path: Option<PathBuf>,
    show_rules: bool,
    bold_fonts: bool,
    clipboard: Option<arboard::Clipboard>,
    tab: Tab,
    grep: GrepState,
}

impl App {
    fn new(explicit_rules: Option<&str>, bold_fonts: bool) -> Self {
        let rules_path = rules_path(explicit_rules);
        let rules_text = load_rules_text(&rules_path);
        let (rules, rules_errors) = Rules::from_text(&rules_text);

        let mut app = Self {
            source: SAMPLE.to_string(),
            output: String::new(),
            errors: Vec::new(),
            live: true,
            status: String::new(),
            rules,
            rules_text,
            rules_errors,
            rules_path,
            show_rules: false,
            bold_fonts,
            clipboard: arboard::Clipboard::new().ok(),
            tab: Tab::Convert,
            grep: GrepState::new(),
        };
        app.convert();
        app
    }

    fn convert(&mut self) {
        let started = std::time::Instant::now();
        let Conversion { code, errors } = convert_with_rules(&self.source, &self.rules);
        self.output = code;
        self.errors = errors;

        let mut status = format!(
            "{} lines in  |  {} lines out  |  {} ms",
            self.source.lines().count(),
            self.output.lines().count(),
            started.elapsed().as_millis()
        );
        if !self.rules.is_empty() {
            status.push_str(&format!("  |  {} rule(s)", self.rules.rule_count()));
        }
        self.status = status;
    }

    fn load_example(&mut self) {
        self.source = SAMPLE.to_string();
        self.convert();
    }

    fn copy_formatted(&mut self, ctx: &egui::Context) {
        let plain = self.output.clone();
        let html = bold_html(&plain);

        if self.clipboard.is_none() {
            self.clipboard = arboard::Clipboard::new().ok();
        }

        let result = self
            .clipboard
            .as_mut()
            .map(|clipboard| clipboard.set_html(html.as_str(), Some(plain.as_str())));

        match result {
            Some(Ok(())) => {
                self.status = "Copied with bold keywords (paste into Word, Docs, etc.)".to_string();
            }
            Some(Err(error)) => {
                ctx.copy_text(plain);
                self.status = format!("Bold copy failed ({error}); copied plain text instead");
            }
            None => {
                ctx.copy_text(plain);
                self.status = "Rich clipboard unavailable; copied plain text".to_string();
            }
        }
    }

    fn copy_rich(
        &mut self,
        ctx: &egui::Context,
        html: String,
        plain: String,
        message: &str,
    ) -> String {
        if self.clipboard.is_none() {
            self.clipboard = arboard::Clipboard::new().ok();
        }

        let result = self
            .clipboard
            .as_mut()
            .map(|clipboard| clipboard.set_html(html.as_str(), Some(plain.as_str())));

        match result {
            Some(Ok(())) => message.to_string(),
            Some(Err(error)) => {
                ctx.copy_text(plain);
                format!("{message}; rich copy failed ({error}), plain text copied")
            }
            None => {
                ctx.copy_text(plain);
                format!("{message}; copied plain text")
            }
        }
    }

    fn start_search(&mut self, ctx: &egui::Context) {
        if self.grep.running {
            return;
        }
        if self.grep.options.pattern.trim().is_empty() {
            self.grep.status = "Enter a search pattern first".to_string();
            return;
        }

        let options = self.grep.options.clone();
        let (tx, rx) = mpsc::channel();
        self.grep.rx = Some(rx);
        self.grep.running = true;
        self.grep.status = "Searching with ripgrep...".to_string();
        self.grep.result = None;

        std::thread::spawn(move || {
            let _ = tx.send(search::run(&options));
        });
        ctx.request_repaint();
    }

    fn poll_search(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.grep.rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(result)) => {
                let mut status = format!(
                    "{} match(es) in {} file(s)  |  {} block(s)",
                    result.match_count,
                    result.files_with_matches,
                    result.blocks.len()
                );
                if !result.warnings.is_empty() {
                    status.push_str(&format!("  |  {} warning(s)", result.warnings.len()));
                }
                self.grep.status = status;
                self.grep.result = Some(result);
                self.grep.running = false;
            }
            Ok(Err(error)) => {
                self.grep.status = error;
                self.grep.running = false;
            }
            Err(TryRecvError::Empty) => {
                self.grep.rx = Some(rx);
                if self.grep.running {
                    ctx.request_repaint_after(Duration::from_millis(120));
                }
            }
            Err(TryRecvError::Disconnected) => {
                self.grep.status = "Search worker stopped unexpectedly".to_string();
                self.grep.running = false;
            }
        }
    }

    fn copy_grep(&mut self, ctx: &egui::Context, preferred: bool) {
        let Some(result) = self.grep.result.as_ref() else {
            self.grep.status = "Run a search first".to_string();
            return;
        };
        let header = self.grep.include_header;
        let (html, plain, message) = if preferred {
            (
                search::blocks_html(result, header),
                search::blocks_tsv(result, header),
                "Copied block output for Excel (HTML table)",
            )
        } else {
            (
                search::classic_html(result, header),
                search::classic_tsv(result, header),
                "Copied classic search output for Excel (HTML table)",
            )
        };
        self.grep.status = self.copy_rich(ctx, html, plain, message);
    }

    fn grep_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut search = false;
        let mut copy_preferred = false;
        let mut copy_classic = false;

        egui::Panel::top("grep_options").show(ui, |ui| {
            ui.add_space(4.0);
            egui::Grid::new("grep_form")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Folder");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.grep.options.root)
                            .desired_width(620.0)
                            .hint_text("Folder or single file to search from the current directory"),
                    );
                    ui.end_row();

                    ui.label("Pattern");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.grep.options.pattern)
                            .desired_width(620.0)
                            .hint_text("Regular expression or literal text"),
                    );
                    ui.end_row();

                    ui.label("Files");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.grep.options.glob)
                            .desired_width(320.0)
                            .hint_text("*.c, *.h  (comma-separated; blank means all files)"),
                    );
                    ui.end_row();

                    ui.label("Options");
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut self.grep.options.fixed_strings, "Fixed text");
                        ui.checkbox(&mut self.grep.options.case_sensitive, "Case sensitive");
                        ui.checkbox(&mut self.grep.options.whole_word, "Whole word");
                        ui.checkbox(&mut self.grep.options.hidden, "Hidden files");
                        ui.checkbox(&mut self.grep.options.no_ignore, "Search ignored files");
                    });
                    ui.end_row();

                    ui.label("Blocks");
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(
                            &mut self.grep.options.expand_preprocessor,
                            "Expand #if ... #endif",
                        );
                        ui.checkbox(
                            &mut self.grep.options.expand_control,
                            "Expand if / loop / switch blocks",
                        );
                        ui.add(
                            egui::Slider::new(&mut self.grep.options.context, 0..=15)
                                .text("context lines"),
                        );
                    });
                    ui.end_row();

                    ui.label("Paths");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.grep.options.windows_paths, "Windows-style paths");
                        ui.checkbox(&mut self.grep.include_header, "Include header row");
                    });
                    ui.end_row();
                });

            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let search_label = if self.grep.running {
                    "Searching..."
                } else {
                    "Search  (Ctrl+Enter)"
                };
                if ui
                    .add_enabled(
                        !self.grep.running,
                        egui::Button::new(search_label).min_size(egui::vec2(150.0, 0.0)),
                    )
                    .clicked()
                {
                    search = true;
                }
                if ui
                    .add_enabled(
                        self.grep.result.is_some(),
                        egui::Button::new("Copy preferred (Excel)"),
                    )
                    .on_hover_text(
                        "Copy File / Lines / Block rows to the clipboard; paste directly into Excel",
                    )
                    .clicked()
                {
                    copy_preferred = true;
                }
                if ui
                    .add_enabled(
                        self.grep.result.is_some(),
                        egui::Button::new("Copy classic (Excel)"),
                    )
                    .on_hover_text(
                        "Copy line-by-line AgentRansack-style rows to the clipboard; paste into Excel",
                    )
                    .clicked()
                {
                    copy_classic = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&self.grep.status).weak());
                });
            });
            ui.add_space(4.0);
        });

        if search {
            self.start_search(&ctx);
        }
        if copy_preferred {
            self.copy_grep(&ctx, true);
        }
        if copy_classic {
            self.copy_grep(&ctx, false);
        }

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let block_label = match &self.grep.result {
                    Some(result) => format!("Preferred blocks ({})", result.blocks.len()),
                    None => "Preferred blocks".to_string(),
                };
                let classic_label = match &self.grep.result {
                    Some(result) => format!("Classic output ({})", result.classic.len()),
                    None => "Classic output".to_string(),
                };
                ui.selectable_value(&mut self.grep.view, GrepView::Preferred, block_label);
                ui.selectable_value(&mut self.grep.view, GrepView::Classic, classic_label);
                ui.separator();
                ui.label(
                    RichText::new("#if..#endif and if/else blocks are merged automatically")
                        .weak(),
                );
            });
            ui.separator();

            let Some(result) = self.grep.result.as_ref() else {
                ui.add_space(20.0);
                ui.label(
                    RichText::new(
                        "Search a folder with ripgrep. Block view merges matching lines into complete #if..#endif, if/else, loop and switch blocks.",
                    )
                    .weak(),
                );
                return;
            };

            match self.grep.view {
                GrepView::Preferred => {
                    if result.blocks.is_empty() {
                        ui.label("No matches. Try a different pattern or enable ignored/hidden files.");
                        return;
                    }
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for row in &result.blocks {
                                let title = RichText::new(format!("{}  [{}]", row.path, row.lines))
                                    .monospace();
                                egui::CollapsingHeader::new(title)
                                    .id_salt((row.path.clone(), row.start, row.end))
                                    .show(ui, |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&row.block).monospace(),
                                            )
                                            .wrap(),
                                        );
                                    });
                            }
                        });
                }
                GrepView::Classic => {
                    if result.classic.is_empty() {
                        ui.label("No matches. Try a different pattern or enable ignored/hidden files.");
                        return;
                    }
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Grid::new("classic_results")
                                .striped(true)
                                .num_columns(10)
                                .show(ui, |ui| {
                                    for heading in [
                                        "Folder", "File", "Size", "Type", "Modified", "Created",
                                        "Accessed", "Matches", "Line", "Text",
                                    ] {
                                        ui.label(RichText::new(heading).strong());
                                    }
                                    ui.end_row();
                                    let limit = 2000usize;
                                    if result.classic.len() > limit {
                                        ui.colored_label(
                                            Color32::from_rgb(255, 190, 110),
                                            format!(
                                                "Showing first {limit} of {} rows; copy includes all rows.",
                                                result.classic.len()
                                            ),
                                        );
                                        ui.end_row();
                                    }
                                    for row in result.classic.iter().take(limit) {
                                        ui.label(&row.folder);
                                        ui.label(&row.file);
                                        ui.label(&row.size);
                                        ui.label(&row.file_type);
                                        ui.label(&row.modified);
                                        ui.label(&row.created);
                                        ui.label(&row.accessed);
                                        ui.label(row.matches.to_string());
                                        ui.label(row.line.to_string());
                                        ui.label(RichText::new(&row.text).monospace());
                                        ui.end_row();
                                    }
                                });
                        });
                }
            }
        });
    }

    fn apply_rules(&mut self) {
        let (rules, errors) = Rules::from_text(&self.rules_text);
        self.rules = rules;
        self.rules_errors = errors;
        self.convert();
        self.status = if self.rules_errors.is_empty() {
            "Equivalent rules applied".to_string()
        } else {
            format!("Rules applied with {} issue(s)", self.rules_errors.len())
        };
    }

    fn save_rules(&mut self) {
        self.apply_rules();

        let Some(path) = self.rules_path.clone() else {
            self.rules_errors
                .push("No rules file location is available".to_string());
            return;
        };

        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            self.rules_errors
                .push(format!("Could not create {}: {e}", parent.display()));
            return;
        }

        match std::fs::write(&path, &self.rules_text) {
            Ok(()) => self.status = format!("Rules saved to {}", path.display()),
            Err(e) => self
                .rules_errors
                .push(format!("Could not save {}: {e}", path.display())),
        }
    }

    fn reset_rules(&mut self) {
        self.rules_text = DEFAULT_RULES.to_string();
        self.apply_rules();
    }

    fn rules_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_rules;
        let mut apply = false;
        let mut save = false;
        let mut reset = false;
        let errors = self.rules_errors.clone();
        let rules_path = self
            .rules_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "not available".to_string());

        egui::Window::new("Custom pseudocode equivalents")
            .open(&mut open)
            .default_width(680.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(
                    "Rewrite generated pseudocode or map C calls to your own wording. \
                     One rule per line.",
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "term INIT => DECLARE\n\
                         call printf => OUTPUT {args}\n\
                         term TO => :=",
                    )
                    .monospace()
                    .weak(),
                );
                ui.separator();

                if !errors.is_empty() {
                    for error in errors.iter().take(6) {
                        ui.colored_label(Color32::from_rgb(255, 130, 130), error);
                    }
                    ui.add_space(4.0);
                }

                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.rules_text)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .desired_rows(16),
                        );
                    });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        apply = true;
                    }
                    if ui.button("Save").clicked() {
                        save = true;
                    }
                    if ui.button("Reset to defaults").clicked() {
                        reset = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(&rules_path).weak())
                            .on_hover_text("Rules file");
                    });
                });
                ui.label(
                    RichText::new(
                        "term and text rules skip string literals. Rules run in the order written.",
                    )
                    .weak(),
                );
            });

        if apply {
            self.apply_rules();
        }
        if save {
            self.save_rules();
        }
        if reset {
            self.reset_rules();
        }
        self.show_rules = open;
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_search(&ctx);

        let keyboard_convert =
            ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
        if keyboard_convert {
            if self.tab == Tab::Grep {
                self.start_search(&ctx);
            } else {
                self.convert();
            }
        }
        let keyboard_copy =
            ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::C));
        if keyboard_copy {
            if self.tab == Tab::Grep {
                self.copy_grep(&ctx, true);
            } else {
                ctx.copy_text(self.output.clone());
                self.status = "Pseudocode copied to clipboard".to_string();
            }
        }

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("CPseudo").heading().strong());
                ui.separator();
                ui.selectable_value(&mut self.tab, Tab::Convert, "C to Pseudocode");
                ui.selectable_value(&mut self.tab, Tab::Grep, "Grep / Blocks");
                ui.separator();

                if self.tab == Tab::Convert {
                    if ui.button("Convert  (Ctrl+Enter)").clicked() {
                        self.convert();
                    }
                    if ui
                        .button("Copy output")
                        .on_hover_text("Copy plain pseudocode to the clipboard")
                        .clicked()
                    {
                        ui.ctx().copy_text(self.output.clone());
                        self.status = "Pseudocode copied to clipboard".to_string();
                    }
                    if ui
                        .button("Copy bold")
                        .on_hover_text("Copy with bold keywords for Word, Docs, etc.")
                        .clicked()
                    {
                        let ctx = ui.ctx().clone();
                        self.copy_formatted(&ctx);
                    }
                    if ui
                        .button("Equivalents...")
                        .on_hover_text("Add custom keyword and call mappings")
                        .clicked()
                    {
                        self.show_rules = true;
                    }
                    if ui.button("Load example").clicked() {
                        self.load_example();
                    }
                    if ui.button("Clear").clicked() {
                        self.source.clear();
                        self.output.clear();
                        self.errors.clear();
                        self.status = "Cleared".to_string();
                    }

                    ui.checkbox(&mut self.live, "Live convert")
                        .on_hover_text("Re-run the converter whenever the C code changes");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.errors.is_empty() {
                            ui.colored_label(Color32::from_rgb(130, 220, 160), "No issues");
                        } else {
                            let response = ui.colored_label(
                                Color32::from_rgb(255, 130, 130),
                                format!("{} issue(s)", self.errors.len()),
                            );
                            let details = self
                                .errors
                                .iter()
                                .take(12)
                                .map(|e| e.to_string())
                                .collect::<Vec<_>>()
                                .join("\n");
                            response.on_hover_text(details);
                        }
                    });
                } else {
                    ui.label(
                        RichText::new(
                            "ripgrep line search with automatic block merging and Excel-ready output",
                        )
                        .weak(),
                    );
                }
            });
            ui.add_space(6.0);
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let text = if self.tab == Tab::Grep {
                    &self.grep.status
                } else {
                    &self.status
                };
                ui.label(RichText::new(text).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let hint = if self.tab == Tab::Grep {
                        "Ctrl+Enter searches  |  Ctrl+Shift+C copies preferred blocks"
                    } else {
                        "Paste C code on the left with Ctrl+V"
                    };
                    ui.label(RichText::new(hint).weak());
                });
            });
        });

        match self.tab {
            Tab::Convert => {
                egui::Panel::left("source_panel")
                    .resizable(true)
                    .default_size(560.0)
                    .min_size(320.0)
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("C source").strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} lines",
                                            self.source.lines().count()
                                        ))
                                        .weak(),
                                    );
                                },
                            );
                        });
                        ui.separator();

                        let mut changed = false;
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let response = ui.add(
                                    egui::TextEdit::multiline(&mut self.source)
                                        .code_editor()
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(30)
                                        .hint_text("Paste C code here..."),
                                );
                                changed = response.changed();
                            });

                        if changed {
                            if self.source.contains('\t') {
                                self.source = self.source.replace('\t', "    ");
                            }
                            if self.live {
                                self.convert();
                            }
                        }
                    });

                egui::CentralPanel::default().show(ui, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Pseudocode").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("Copy bold")
                                .on_hover_text("Copy with bold keywords for Word, Docs, etc.")
                                .clicked()
                            {
                                let ctx = ui.ctx().clone();
                                self.copy_formatted(&ctx);
                            }
                            if ui.small_button("Copy").clicked() {
                                ui.ctx().copy_text(self.output.clone());
                                self.status = "Pseudocode copied to clipboard".to_string();
                            }
                        });
                    });
                    ui.separator();

                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let bold_fonts = self.bold_fonts;
                            let mut layouter =
                                move |ui: &egui::Ui,
                                      buffer: &dyn egui::TextBuffer,
                                      wrap_width: f32| {
                                    let job = pseudocode_layout_job(
                                        ui,
                                        buffer.as_str(),
                                        wrap_width,
                                        bold_fonts,
                                    );
                                    ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
                                };
                            let mut output = self.output.as_str();
                            ui.add(
                                egui::TextEdit::multiline(&mut output)
                                    .code_editor()
                                    .layouter(&mut layouter)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(30),
                            );
                        });
                });
            }
            Tab::Grep => self.grep_ui(ui),
        }

        if self.show_rules {
            self.rules_window(&ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{bold_html, is_keyword};

    #[test]
    fn keywords_are_bold_in_html() {
        let html = bold_html("INIT int x TO 1\nIF (a AND b)\n");
        assert!(html.contains("<b>INIT</b> int x <b>TO</b> 1"), "{html}");
        assert!(html.contains("<b>IF</b> (a <b>AND</b> b)"), "{html}");
        assert!(is_keyword("IS"));
        assert!(is_keyword("CALL"));
        assert!(!is_keyword("VIO_Success"));
    }

    #[test]
    fn html_is_escaped() {
        let html = bold_html("IF (a < b && c > d)\n");
        assert!(html.contains("&lt;"), "{html}");
        assert!(html.contains("&gt;"), "{html}");
    }
}
