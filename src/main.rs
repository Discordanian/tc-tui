mod globe;

use chrono::{DateTime, Utc};
use chrono_tz::{America::Chicago, Europe::Madrid};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};
use std::io::{self, stdout, Stdout};

type Terminal = ratatui::Terminal<CrosstermBackend<Stdout>>;

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal) -> io::Result<()> {
    let mut rotation: f64 = 0.0;

    loop {
        terminal.draw(|frame| ui(frame, rotation))?;

        rotation += 0.0005;
        if rotation > std::f64::consts::TAU {
            rotation -= std::f64::consts::TAU;
        }

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

fn ui(frame: &mut Frame, rotation: f64) {
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
            Constraint::Ratio(1, 4),
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

    // Left panel: rotating ASCII globe
    let globe_block = Block::default()
        .title(" Globe ")
        .borders(Borders::ALL);
    let globe_inner = globe_block.inner(body_chunks[0]);
    let globe_text = globe::render_globe(
        globe_inner.width as usize,
        globe_inner.height as usize,
        rotation,
    );
    let globe_para = Paragraph::new(globe_text)
        .block(globe_block)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(globe_para, body_chunks[0]);

    // Main content
    let block = Block::default()
        .title(" Tangential Cold TUI ")
        .borders(Borders::ALL);

    let paragraph = Paragraph::new("Welcome! Press 'q' to quit.")
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, body_chunks[1]);
}
