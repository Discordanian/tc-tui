mod config;
mod weather;

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
use std::collections::VecDeque;
use std::io::{self, stdout, Stdout};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::System;
use config::Config;
use weather::{spawn_weather_fetcher, WeatherInfo};

const BAR_GRAPH_HEIGHT: u16 = 3;

type Terminal = ratatui::Terminal<CrosstermBackend<Stdout>>;

fn fetch_status(url: &str) -> String {
    match ureq::get(url).call() {
        Ok(resp) => resp.status().to_string(),
        Err(ureq::Error::Status(code, _)) => code.to_string(),
        Err(_) => "ERR".to_string(),
    }
}

fn refresh_statuses(statuses: &Arc<Mutex<Vec<(String, String)>>>) {
    let urls: Vec<String> = statuses
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|(_, url)| url.clone())
        .collect();

    let results: Vec<(String, String)> = urls
        .iter()
        .map(|url| {
            let status = fetch_status(url);
            (status, url.clone())
        })
        .collect();

    if let Ok(mut s) = statuses.lock() {
        *s = results;
    }
}

fn spawn_status_checker(
    statuses: Arc<Mutex<Vec<(String, String)>>>,
    refresh_rx: mpsc::Receiver<()>,
    interval_secs: u64,
) {
    thread::spawn(move || {
        loop {
            refresh_statuses(&statuses);

            match refresh_rx.recv_timeout(Duration::from_secs(interval_secs)) {
                Ok(()) => continue,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}

fn main() -> io::Result<()> {
    let cfg = config::load();

    let statuses: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(
        cfg.urls.sites.iter().map(|url| ("...".to_string(), url.clone())).collect(),
    ));

    let weather: Arc<Mutex<Vec<WeatherInfo>>> = Arc::new(Mutex::new(
        cfg.locations.iter().map(|l| WeatherInfo::pending(&l.label)).collect(),
    ));

    let (status_refresh_tx, status_refresh_rx) = mpsc::channel();
    let (weather_refresh_tx, weather_refresh_rx) = mpsc::channel();

    spawn_status_checker(Arc::clone(&statuses), status_refresh_rx, cfg.refresh.url_check_secs);
    spawn_weather_fetcher(Arc::clone(&weather), weather_refresh_rx, cfg.locations.clone(), cfg.refresh.weather_secs);

    // All refresh senders — add new ones here as panels are added
    let refresh_senders: Vec<mpsc::Sender<()>> = vec![status_refresh_tx, weather_refresh_tx];

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &statuses, &weather, &refresh_senders, &cfg);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

struct SysSnapshot {
    cpu_count: usize,
    total_ram: f64,
    used_ram: f64,
    cpu_load: f32,
}

fn take_sys_snapshot(sys: &System) -> SysSnapshot {
    let cpu_load = if sys.cpus().is_empty() {
        0.0
    } else {
        sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / sys.cpus().len() as f32
    };
    SysSnapshot {
        cpu_count: sys.cpus().len(),
        total_ram: sys.total_memory() as f64 / 1_073_741_824.0,
        used_ram: sys.used_memory() as f64 / 1_073_741_824.0,
        cpu_load,
    }
}

fn current_cpu_load(sys: &System) -> f32 {
    if sys.cpus().is_empty() {
        0.0
    } else {
        sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / sys.cpus().len() as f32
    }
}

fn run_app(
    terminal: &mut Terminal,
    statuses: &Arc<Mutex<Vec<(String, String)>>>,
    weather: &Arc<Mutex<Vec<WeatherInfo>>>,
    refresh_senders: &[mpsc::Sender<()>],
    cfg: &Config,
) -> io::Result<()> {
    let mut sys = System::new_all();
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let mut sys_snapshot = take_sys_snapshot(&sys);
    let cpu_history_len = cfg.display.cpu_history_len;
    let cpu_sample_secs = cfg.refresh.cpu_sample_secs;

    let mut cpu_history: VecDeque<f32> = VecDeque::with_capacity(cpu_history_len);
    let mut last_bar_sample = Instant::now();

    loop {
        let now = Instant::now();

        sys.refresh_cpu_usage();

        if now.duration_since(last_bar_sample) >= Duration::from_secs(cpu_sample_secs) {
            sys.refresh_memory();
            let load = current_cpu_load(&sys);
            if cpu_history.len() >= cpu_history_len {
                cpu_history.pop_front();
            }
            cpu_history.push_back(load);
            sys_snapshot = take_sys_snapshot(&sys);
            last_bar_sample = now;
        }

        let status_data: Vec<(String, String)> = statuses
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        let history: Vec<f32> = cpu_history.iter().copied().collect();
        let weather_data: Vec<WeatherInfo> = weather
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        terminal.draw(|frame| ui(frame, &status_data, &sys_snapshot, &history, &weather_data, cpu_history_len))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('r') => {
                            for tx in refresh_senders {
                                let _ = tx.send(());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn ui(frame: &mut Frame, statuses: &[(String, String)], sys: &SysSnapshot, cpu_history: &[f32], weather: &[WeatherInfo], cpu_history_len: usize) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Min(0),
        ])
        .split(chunks[1]);

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(statuses.len() as u16 + 3),
            Constraint::Length(6 + 2),
            Constraint::Length(BAR_GRAPH_HEIGHT + 3),
            Constraint::Min(0),
        ])
        .split(body_chunks[0]);

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
                c if c.starts_with('4') || c.starts_with('5') => Style::default().fg(Color::Red),
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
            .title(" URL Status ")
            .borders(Borders::ALL),
    );

    frame.render_widget(table, left_chunks[0]);

    // Left panel: system info
    let sys_rows = vec![
        Row::new(vec![
            Cell::from("CPU Count"),
            Cell::from(format!("{}", sys.cpu_count)),
        ]),
        Row::new(vec![
            Cell::from("RAM Total"),
            Cell::from(format!("{:.1} GB", sys.total_ram)),
        ]),
        Row::new(vec![
            Cell::from("RAM Usage"),
            Cell::from(format!("{:.1} / {:.1} GB", sys.used_ram, sys.total_ram)),
        ]),
        Row::new(vec![
            Cell::from("CPU Load"),
            Cell::from(format!("{:.1}%", sys.cpu_load)),
        ]),
    ];

    let sys_table = Table::new(
        sys_rows,
        [Constraint::Length(12), Constraint::Min(0)],
    )
    .block(
        Block::default()
            .title(" System ")
            .borders(Borders::ALL),
    )
    .style(Style::default().fg(Color::Cyan));

    frame.render_widget(sys_table, left_chunks[1]);

    // Left panel: CPU bar graph
    let graph_block = Block::default()
        .title(" CPU History ")
        .borders(Borders::ALL);
    let graph_inner = graph_block.inner(left_chunks[2]);
    frame.render_widget(graph_block, left_chunks[2]);

    let w = graph_inner.width as usize;
    let bar_width = (w / cpu_history_len).max(1);
    let gap = if bar_width > 1 { 1 } else { 0 };
    let fill = bar_width.saturating_sub(gap);
    let buf = frame.buffer_mut();

    for (i, &load) in cpu_history.iter().enumerate() {
        let filled_boxes = ((load / 33.34).ceil() as u16).min(BAR_GRAPH_HEIGHT);

        let x = graph_inner.x + (i * bar_width) as u16;
        if x >= graph_inner.x + graph_inner.width {
            break;
        }
        let avail = ((graph_inner.x + graph_inner.width) - x) as usize;
        let draw_width = fill.min(avail);

        for row in 0..BAR_GRAPH_HEIGHT {
            let y = graph_inner.y + (BAR_GRAPH_HEIGHT - 1 - row);
            if y >= graph_inner.y + graph_inner.height {
                continue;
            }

            let row_color = match row {
                0 => Color::Green,
                1 => Color::Yellow,
                _ => Color::Red,
            };
            let span = if row < filled_boxes {
                Span::styled("\u{2588}".repeat(draw_width), Style::default().fg(row_color))
            } else {
                Span::raw(" ".repeat(draw_width))
            };
            buf.set_span(x, y, &span, draw_width as u16);
        }
    }

    // Right panel: split into weather (top) and main content (bottom)
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(0),
        ])
        .split(body_chunks[1]);

    // Weather box
    let weather_lines: Vec<Line> = weather.iter().map(|w| {
        Line::from(vec![
            Span::styled(
                format!("{:<12}", w.city),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                " {:>5.1}°F ({:>5.1}°C)  H:{:.1}°F ({:.1}°C)  L:{:.1}°F ({:.1}°C)  {} {}",
                w.current_f, w.current_c,
                w.high_f, w.high_c,
                w.low_f, w.low_c,
                w.emoji, w.description,
            )),
        ])
    }).collect();

    let weather_para = Paragraph::new(weather_lines)
        .block(Block::default().title(" Weather ").borders(Borders::ALL));
    frame.render_widget(weather_para, right_chunks[0]);

    // Main content
    let block = Block::default()
        .title(" Tangential Cold TUI ")
        .borders(Borders::ALL);

    let paragraph = Paragraph::new("Welcome! Press 'q' to quit.")
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, right_chunks[1]);

    // Bottom menu bar
    let menu = Line::from(vec![
        Span::styled(" q", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(" Quit  "),
        Span::styled("r", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(" Refresh"),
    ]);
    let menu_bar = Paragraph::new(menu)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(menu_bar, chunks[2]);
}
