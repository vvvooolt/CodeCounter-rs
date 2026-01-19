use std::error::Error;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;
use ratatui::{backend::CrosstermBackend, prelude::Frame};
use walkdir::WalkDir;

const DIGIT_HEIGHT: usize = 5;
const DIGITS: [[&str; DIGIT_HEIGHT]; 10] = [
    [" ███ ", "█   █", "█   █", "█   █", " ███ "],
    ["  █  ", " ██  ", "  █  ", "  █  ", " ███ "],
    [" ███ ", "█   █", "   █ ", "  █  ", "█████"],
    [" ███ ", "█   █", "  ██ ", "█   █", " ███ "],
    ["█   █", "█   █", "█████", "    █", "    █"],
    ["█████", "█    ", "████ ", "    █", "████ "],
    [" ███ ", "█    ", "████ ", "█   █", " ███ "],
    ["█████", "    █", "   █ ", "  █  ", "  █  "],
    [" ███ ", "█   █", " ███ ", "█   █", " ███ "],
    [" ███ ", "█   █", " ████", "    █", " ███ "],
];
const COMMA: [&str; DIGIT_HEIGHT] = ["   ", "   ", "   ", " █ ", "█  "];
const SCALE_X: usize = 4;
const SCALE_Y: usize = 2;
const CODE_EXTENSIONS: &[&str] = &[
    "rs", "py", "pyw", "pyi", "ipynb", "js", "mjs", "cjs", "jsm", "ts", "mts", "cts", "jsx", "tsx",
    "java", "kt", "kts", "groovy", "gradle", "gvy", "gy", "gsh", "scala", "sc", "sbt", "swift",
    "c", "h", "cc", "cxx", "cpp", "hpp", "hh", "hxx", "inl", "ipp", "tpp", "inc", "idl", "d", "di",
    "m", "mm", "go", "rs", "zig", "nim", "nimble", "v", "cr", "hs", "lhs", "ml", "mli", "mll",
    "mly", "re", "rei", "fs", "fsi", "fsx", "fsproj", "cs", "csx", "vb", "vbs", "bas", "pas",
    "rb", "erb", "rake", "gemspec", "php", "phtml", "phpt", "twig", "blade", "pl", "pm", "r", "rmd",
    "jl", "dart", "elm", "clj", "cljs", "cljc", "edn", "ex", "exs", "erl", "hrl", "lua", "nu",
    "sh", "bash", "zsh", "fish", "ps1", "psm1", "psd1", "bat", "cmd", "asm", "s", "sql", "psql",
    "pgsql", "mysql", "sqlite", "sqlite3", "ddl", "dml", "proto", "thrift", "avsc", "avdl",
    "graphql", "gql", "prisma", "tf", "tfvars", "hcl", "cue", "rego",
    "html", "htm", "xhtml", "xml", "xsd", "xsl", "xslt",
    "css", "scss", "sass", "less", "styl", "stylus", "postcss",
    "md", "mdx", "markdown", "rst", "adoc", "asciidoc", "org",
    "tex", "latex", "sty", "cls", "bib",
    "toml", "yaml", "yml", "json", "jsonc", "json5", "ini", "cfg", "conf", "properties", "env",
    "make", "mk", "cmake",
    "vue", "svelte", "astro",
];

#[derive(Debug)]
struct ScanResult {
    lines: u64,
    files: u64,
    dir: PathBuf,
    scanned_at: DateTime<Local>,
}

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new()?;
    let res = app.run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

struct App {
    scan: ScanResult,
    last_scan: Instant,
}

impl App {
    fn new() -> Result<Self, Box<dyn Error>> {
        let scan = scan_directory(std::env::current_dir()?)?;
        Ok(Self {
            scan,
            last_scan: Instant::now(),
        })
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), Box<dyn Error>> {
        loop {
            terminal.draw(|frame| draw_ui(frame, self))?;

            if event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::Enter => self.refresh()?,
                        _ => {}
                    }
                }
            }
        }
    }

    fn refresh(&mut self) -> Result<(), Box<dyn Error>> {
        let scan = scan_directory(self.scan.dir.clone())?;
        self.scan = scan;
        self.last_scan = Instant::now();
        Ok(())
    }
}

fn draw_ui(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let timestamp = app.scan.scanned_at.format("%Y-%m-%d %H:%M:%S %z");
    let headline = Paragraph::new(Line::from(format!(
        "As of {} the number of lines of code in this repo is:",
        timestamp
    )))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL));

    let ascii_lines = ascii_art_number(app.scan.lines);
    let time_line = format!(
        "Time since last scan: {}",
        format_duration(app.last_scan.elapsed())
    );
    let ascii_width = ascii_lines
        .iter()
        .map(|line| line.chars().count())
        .chain(std::iter::once(time_line.chars().count()))
        .max()
        .unwrap_or(0) as u16;
    let ascii_height = ascii_lines.len() as u16 + 2;
    let mut ascii_text = ascii_lines
        .iter()
        .map(|line| Line::styled(line.clone(), Style::default().fg(Color::Cyan)))
        .collect::<Vec<_>>();
    ascii_text.push(Line::from(""));
    ascii_text.push(Line::from(time_line));

    let ascii = Paragraph::new(ascii_text)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Lines of Code"));

    let info = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Directory: ", Style::default().fg(Color::Yellow)),
            Span::raw(app.scan.dir.display().to_string()),
        ]),
        Line::from(vec![
            Span::styled("Files scanned: ", Style::default().fg(Color::Yellow)),
            Span::raw(app.scan.files.to_string()),
        ]),
        Line::from("Keys: r/R/Enter = rescan, q/Q/Esc = quit."),
    ])
    .block(Block::default().borders(Borders::ALL))
    .wrap(Wrap { trim: true });

    let header_height = 3u16.min(area.height);
    let info_height = 5u16.min(area.height);
    let header_rect = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: header_height,
    };
    let info_rect = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(info_height),
        width: area.width,
        height: info_height,
    };
    let ascii_rect = centered_rect(
        ascii_width.saturating_add(2),
        ascii_height.saturating_add(2),
        area,
    );

    frame.render_widget(headline, header_rect);
    frame.render_widget(info, info_rect);
    frame.render_widget(ascii, ascii_rect);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn ascii_art_number(value: u64) -> Vec<String> {
    let chars = format_with_commas(value).chars().collect::<Vec<_>>();
    let mut lines = Vec::with_capacity(DIGIT_HEIGHT);
    for row in 0..DIGIT_HEIGHT {
        let mut line = String::new();
        for (idx, ch) in chars.iter().enumerate() {
            if idx > 0 {
                let prev = chars[idx - 1];
                if *ch != ',' && prev != ',' {
                    line.push_str("  ");
                }
            }
            let pattern = match ch {
                '0'..='9' => DIGITS[ch.to_digit(10).unwrap() as usize][row],
                ',' => COMMA[row],
                _ => "     ",
            };
            line.push_str(&expand_scaled_row(pattern));
        }
        for _ in 0..SCALE_Y {
            lines.push(line.clone());
        }
    }
    lines
}

fn format_with_commas(value: u64) -> String {
    let raw = value.to_string();
    let mut out = String::with_capacity(raw.len() + raw.len() / 3);
    let mut count = 0;
    for ch in raw.chars().rev() {
        if count == 3 {
            out.push(',');
            count = 0;
        }
        out.push(ch);
        count += 1;
    }
    out.chars().rev().collect()
}

fn format_duration(duration: Duration) -> String {
    let total_ms = duration.as_millis();
    let ms_in_day = 86_400_000u128;
    let ms_in_hour = 3_600_000u128;
    let ms_in_minute = 60_000u128;
    let ms_in_second = 1_000u128;

    let days = total_ms / ms_in_day;
    let hours = (total_ms % ms_in_day) / ms_in_hour;
    let minutes = (total_ms % ms_in_hour) / ms_in_minute;
    let seconds = (total_ms % ms_in_minute) / ms_in_second;
    let millis = total_ms % ms_in_second;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 {
        parts.push(format!("{}m", minutes));
    }
    parts.push(format!("{}s", seconds));
    if millis > 0 || parts.is_empty() {
        parts.push(format!("{}ms", millis));
    }

    parts.join(" ")
}

fn expand_scaled_row(row: &str) -> String {
    let mut out = String::with_capacity(row.len() * SCALE_X);
    for ch in row.chars() {
        match ch {
            '█' => {
                for _ in 0..SCALE_X {
                    out.push('█');
                }
            }
            _ => {
                for _ in 0..SCALE_X {
                    out.push(' ');
                }
            }
        }
    }
    out
}

fn scan_directory(dir: PathBuf) -> Result<ScanResult, Box<dyn Error>> {
    let mut lines = 0u64;
    let mut files = 0u64;

    let walker = WalkDir::new(&dir).into_iter().filter_entry(|entry| !is_ignored(entry.path()));

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        if entry.file_type().is_file() && is_code_file(entry.path()) {
            files += 1;
            lines += count_lines(entry.path()).unwrap_or(0);
        }
    }

    Ok(ScanResult {
        lines,
        files,
        dir,
        scanned_at: Local::now(),
    })
}

fn is_ignored(path: &Path) -> bool {
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy();
        if name == ".git" || name == "target" || name == "node_modules" {
            return true;
        }
    }
    false
}

fn count_lines(path: &Path) -> io::Result<u64> {
    let mut file = File::open(path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    if buf.is_empty() {
        return Ok(0);
    }

    let mut count = buf.iter().filter(|b| **b == b'\n').count() as u64;
    if *buf.last().unwrap() != b'\n' {
        count += 1;
    }
    Ok(count)
}

fn is_code_file(path: &Path) -> bool {
    let ext = match path.extension() {
        Some(ext) => ext.to_string_lossy().to_lowercase(),
        None => return false,
    };
    CODE_EXTENSIONS.iter().any(|allowed| *allowed == ext)
}
