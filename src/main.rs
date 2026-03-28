use chrono::{DateTime, Utc};
use chrono_tz::{America::Chicago, Europe::Madrid};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};
use std::io::{self, stdout, Stdout};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const URLS: &[&str] = &[
    "https://tangentialcold.com",
    "https://babilonia.tangentialcold.com",
    "https://annaschwind.com",
    "https://slithytoves.org",
];

type Terminal = ratatui::Terminal<CrosstermBackend<Stdout>>;

fn fetch_status(url: &str) -> String {
    match ureq::get(url).call() {
        Ok(resp) => resp.status().to_string(),
        Err(ureq::Error::Status(code, _)) => code.to_string(),
        Err(_) => "ERR".to_string(),
    }
}

fn spawn_status_checker(statuses: Arc<Mutex<Vec<(String, String)>>>) {
    thread::spawn(move || {
        loop {
            let results: Vec<(String, String)> = URLS
                .iter()
                .map(|url| {
                    let status = fetch_status(url);
                    (status, url.to_string())
                })
                .collect();

            if let Ok(mut s) = statuses.lock() {
                *s = results;
            }

            thread::sleep(Duration::from_secs(180));
        }
    });
}

fn main() -> io::Result<()> {
    let statuses: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(
        URLS.iter().map(|url| ("...".to_string(), url.to_string())).collect(),
    ));

    spawn_status_checker(Arc::clone(&statuses));

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &statuses);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal, statuses: &Arc<Mutex<Vec<(String, String)>>>) -> io::Result<()> {
    loop {
        let status_data: Vec<(String, String)> = statuses
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        terminal.draw(|frame| ui(frame, &status_data))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        _ => {}
                    }
                }
            }
        }
    }
}

fn ui(frame: &mut Frame, statuses: &[(String, String)]) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(area);

    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Min(0),
        ])
        .split(chunks[1]);

    // Header bar
    let now: DateTime<Utc> = Utc::now();
    let spain_time = now.with_timezone(&Madrid).format("%H:%M").to_string();
    let stlouis_time = now.with_timezone(&Chicago).format("%H:%M").to_string();
    let date = now.with_timezone(&Madrid).format("%Y-%m-%d").to_string();

    let hostname = hostname::get().unwrap_or_else(|_| std::ffi::OsString::from("unknown"));

    let header_block = Block::default().borders(Borders::BOTTOM);
    let header_inner = header_block.inner(chunks[0]);

    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(hostname.len() as u16 + 2),
            Constraint::Min(0),
        ])
        .split(header_inner);

    let times_text = format!("🇪🇸 {} │ 🇺🇸 {}", spain_time, stlouis_time);
    let times = Paragraph::new(times_text)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(times, header_chunks[0]);

    let hostname_para = Paragraph::new(hostname.to_string_lossy().to_string())
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center);
    frame.render_widget(hostname_para, header_chunks[1]);

    let date_para = Paragraph::new(date)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Right);
    frame.render_widget(date_para, header_chunks[2]);

    frame.render_widget(header_block, chunks[0]);

    // Left panel: status table
    let rows: Vec<Row> = statuses
        .iter()
        .map(|(code, url)| {
            let style = match code.as_str() {
                "200" => Style::default().fg(Color::Green),
                c if c.starts_with('3') => Style::default().fg(Color::Yellow),
                c if c.starts_with('4') | c.starts_with('5') => Style::default().fg(Color::Red),
                "..." => Style::default().fg(Color::DarkGray),
                _ => Style::default().fg(Color::Red),
            };
            Row::new(vec![
                Cell::from(code.clone()).style(style),
                Cell::from(url.clone()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [Constraint::Length(5), Constraint::Min(0)],
    )
    .header(
        Row::new(vec!["Code", "URL"])
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    )
    .block(
        Block::default()
            .title(" Status ")
            .borders(Borders::ALL),
    );

    frame.render_widget(table, body_chunks[0]);

    // Main content
    let block = Block::default()
        .title(" Tangential Cold TUI ")
        .borders(Borders::ALL);

    let paragraph = Paragraph::new("Welcome! Press 'q' to quit.")
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, body_chunks[1]);
}
