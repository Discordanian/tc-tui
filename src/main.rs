mod config;
mod currency;
mod github;
mod system;
mod weather;

use chrono::{DateTime, Datelike, Utc, Weekday};
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
use config::{Config, ConfigSource};
use currency::{spawn_currency_fetcher, CurrencyRate};
use github::{spawn_github_fetcher, GitHubActivity};
use system::{current_cpu_load, render_system_table, take_snapshot, SysSnapshot, SYSTEM_TABLE_HEIGHT};
use weather::{spawn_weather_fetcher, WeatherInfo};

const BAR_GRAPH_HEIGHT: u16 = 3;
const CURRENCY_BOX_HEIGHT: u16 = 4;

type Terminal = ratatui::Terminal<CrosstermBackend<Stdout>>;

struct RunAppModel<'a> {
    statuses: &'a Arc<Mutex<Vec<(String, String)>>>,
    weather: &'a Arc<Mutex<Vec<WeatherInfo>>>,
    ip_city: &'a Arc<Mutex<String>>,
    currency_rates: &'a Arc<Mutex<(CurrencyRate, CurrencyRate)>>,
    github_activity: &'a Arc<Mutex<GitHubActivity>>,
    refresh_senders: &'a [mpsc::Sender<()>],
    cfg: &'a Config,
    cfg_source: &'a ConfigSource,
}

struct UiModel<'a> {
    statuses: &'a [(String, String)],
    sys: &'a SysSnapshot,
    cpu_history: &'a [f32],
    weather: &'a [WeatherInfo],
    ip_city: &'a str,
    vpn: bool,
    cpu_history_len: usize,
    cfg_source: &'a ConfigSource,
    currency_inputs: &'a [String; 2],
    active_currency_input: usize,
    currency_rates: &'a (CurrencyRate, CurrencyRate),
    github_activity: &'a GitHubActivity,
}

fn fetch_ip_city() -> String {
    #[derive(serde::Deserialize)]
    struct IpInfo {
        city: Option<String>,
        region: Option<String>,
    }
    match ureq::get("https://ipinfo.io/json").call() {
        Ok(resp) => match resp.into_json::<IpInfo>() {
            Ok(info) => match (info.city, info.region) {
                (Some(c), Some(r)) => format!("{}, {}", c, r),
                (Some(c), None) => c,
                _ => "Unknown".to_string(),
            },
            Err(_) => "Unknown".to_string(),
        },
        Err(_) => "Unknown".to_string(),
    }
}

fn spawn_ip_city_fetcher(
    city: Arc<Mutex<String>>,
    refresh_rx: mpsc::Receiver<()>,
    interval_secs: u64,
) {
    thread::spawn(move || {
        loop {
            let result = fetch_ip_city();
            if let Ok(mut c) = city.lock() {
                *c = result;
            }
            match refresh_rx.recv_timeout(Duration::from_secs(interval_secs)) {
                Ok(()) => continue,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}

fn vpn_active() -> bool {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .iter()
        .any(|iface| {
            let n = iface.name.as_str();
            n.starts_with("tun") || n.starts_with("tap") || n.starts_with("utun")
                || n.starts_with("wg") || n.starts_with("ppp")
        })
}

fn fetch_status(url: &str) -> String {
    match ureq::builder()
        .timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .get(url)
        .call()
    {
        Ok(resp) => resp.status().to_string(),
        Err(ureq::Error::Status(code, _)) => code.to_string(),
        Err(_) => "ERR".to_string(),
    }
}

fn reset_statuses(statuses: &Arc<Mutex<Vec<(String, String)>>>) {
    if let Ok(mut s) = statuses.lock() {
        for (code, _) in s.iter_mut() {
            *code = "...".to_string();
        }
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
    let (cfg, cfg_source) = config::load();
    let (currency_a, currency_b) = currency_units(&cfg);

    let statuses: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(
        cfg.urls.sites.iter().map(|url| ("...".to_string(), url.clone())).collect(),
    ));

    let weather: Arc<Mutex<Vec<WeatherInfo>>> = Arc::new(Mutex::new(
        cfg.locations.iter().map(|l| WeatherInfo::pending(&l.label)).collect(),
    ));

    let ip_city: Arc<Mutex<String>> = Arc::new(Mutex::new("...".to_string()));
    let currency_rates: Arc<Mutex<(CurrencyRate, CurrencyRate)>> = Arc::new(Mutex::new((
        CurrencyRate::pending(&currency_a, &currency_b),
        CurrencyRate::pending(&currency_b, &currency_a),
    )));

    let github_activity: Arc<Mutex<GitHubActivity>> =
        Arc::new(Mutex::new(GitHubActivity::pending()));

    let (status_refresh_tx, status_refresh_rx) = mpsc::channel();
    let (weather_refresh_tx, weather_refresh_rx) = mpsc::channel();
    let (ip_city_refresh_tx, ip_city_refresh_rx) = mpsc::channel();
    let (currency_refresh_tx, currency_refresh_rx) = mpsc::channel();
    let (github_refresh_tx, github_refresh_rx) = mpsc::channel();

    spawn_status_checker(Arc::clone(&statuses), status_refresh_rx, cfg.refresh.url_check_secs);
    spawn_weather_fetcher(Arc::clone(&weather), weather_refresh_rx, cfg.locations.clone(), cfg.refresh.weather_secs);
    spawn_ip_city_fetcher(Arc::clone(&ip_city), ip_city_refresh_rx, 300);
    spawn_currency_fetcher(
        Arc::clone(&currency_rates),
        currency_refresh_rx,
        currency_a.clone(),
        currency_b.clone(),
        cfg.refresh.currency_secs,
    );
    spawn_github_fetcher(
        Arc::clone(&github_activity),
        github_refresh_rx,
        cfg.github.username.clone(),
        cfg.refresh.github_secs,
    );

    // All refresh senders — add new ones here as panels are added
    let refresh_senders: Vec<mpsc::Sender<()>> =
        vec![status_refresh_tx, weather_refresh_tx, ip_city_refresh_tx, currency_refresh_tx, github_refresh_tx];

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(
        &mut terminal,
        RunAppModel {
            statuses: &statuses,
            weather: &weather,
            ip_city: &ip_city,
            currency_rates: &currency_rates,
            github_activity: &github_activity,
            refresh_senders: &refresh_senders,
            cfg: &cfg,
            cfg_source: &cfg_source,
        },
    );

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal, model: RunAppModel<'_>) -> io::Result<()> {
    let RunAppModel {
        statuses,
        weather,
        ip_city,
        currency_rates,
        github_activity,
        refresh_senders,
        cfg,
        cfg_source,
    } = model;
    let mut sys = System::new_all();
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let mut sys_snapshot = take_snapshot(&sys);
    let cpu_history_len = cfg.display.cpu_history_len;
    let cpu_sample_secs = cfg.refresh.cpu_sample_secs;

    let mut cpu_history: VecDeque<f32> = VecDeque::with_capacity(cpu_history_len);
    let mut last_bar_sample = Instant::now();

    let vpn_refresh_secs = 300;
    let mut is_vpn_active = vpn_active();
    let mut last_vpn_check = Instant::now();
    let mut currency_inputs = [String::from("1"), String::from("1")];
    let mut active_currency_input = 0usize;

    loop {
        let now = Instant::now();

        sys.refresh_cpu_usage();

        if now.duration_since(last_vpn_check) >= Duration::from_secs(vpn_refresh_secs) {
            is_vpn_active = vpn_active();
            last_vpn_check = now;
        }

        if now.duration_since(last_bar_sample) >= Duration::from_secs(cpu_sample_secs) {
            sys.refresh_memory();
            let load = current_cpu_load(&sys);
            if cpu_history.len() >= cpu_history_len {
                cpu_history.pop_front();
            }
            cpu_history.push_back(load);
            sys_snapshot = take_snapshot(&sys);
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
        let city = ip_city.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let rate_data = currency_rates
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let gh_data = github_activity
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        terminal.draw(|frame| {
            ui(
                frame,
                UiModel {
                    statuses: &status_data,
                    sys: &sys_snapshot,
                    cpu_history: &history,
                    weather: &weather_data,
                    ip_city: &city,
                    vpn: is_vpn_active,
                    cpu_history_len,
                    cfg_source,
                    currency_inputs: &currency_inputs,
                    active_currency_input,
                    currency_rates: &rate_data,
                    github_activity: &gh_data,
                },
            )
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('r') => {
                            reset_statuses(statuses);
                            for tx in refresh_senders {
                                let _ = tx.send(());
                            }
                            last_bar_sample -= Duration::from_secs(cpu_sample_secs);
                            is_vpn_active = vpn_active();
                            last_vpn_check = now;
                        }
                        KeyCode::Tab | KeyCode::Down | KeyCode::Up => {
                            active_currency_input = (active_currency_input + 1) % 2;
                        }
                        KeyCode::Backspace => {
                            currency_inputs[active_currency_input].pop();
                        }
                        KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                            currency_inputs[active_currency_input].push(c);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn ui(frame: &mut Frame, m: UiModel<'_>) {
    let UiModel {
        statuses,
        sys,
        cpu_history,
        weather,
        ip_city,
        vpn,
        cpu_history_len,
        cfg_source,
        currency_inputs,
        active_currency_input,
        currency_rates,
        github_activity,
    } = m;
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
            Constraint::Length(SYSTEM_TABLE_HEIGHT),
            Constraint::Length(BAR_GRAPH_HEIGHT + 2),
            Constraint::Length(CURRENCY_BOX_HEIGHT),
            Constraint::Min(0),
        ])
        .split(body_chunks[0]);

    // Header bar
    let now: DateTime<Utc> = Utc::now();
    let spain_time = now.with_timezone(&Madrid).format("%H:%M").to_string();
    let stlouis_time = now.with_timezone(&Chicago).format("%H:%M").to_string();
    let day_date = format_header_day_date(now);

    let hostname = hostname::get().unwrap_or_else(|_| std::ffi::OsString::from("unknown"));
    let lock = if vpn { "🔒" } else { "🔓" };
    // "(city) 🔒 hostname" — emoji is 2 cols, city parens + space + space = city.len()+4
    let center_text = format!("({}) {} {}", ip_city, lock, hostname.to_string_lossy());
    let center_width = (ip_city.len() + 2 + 1 + 2 + 1 + hostname.len()) as u16 + 2;

    let header_block = Block::default().borders(Borders::BOTTOM);
    let header_inner = header_block.inner(chunks[0]);

    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(center_width),
            Constraint::Min(0),
        ])
        .split(header_inner);

    let times_text = format!("🇪🇸 {} │ 🇺🇸 {}", spain_time, stlouis_time);
    let times = Paragraph::new(times_text)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(times, header_chunks[0]);

    let hostname_para = Paragraph::new(center_text)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center);
    frame.render_widget(hostname_para, header_chunks[1]);

    let date_para = Paragraph::new(day_date)
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

    render_system_table(frame, left_chunks[1], sys);

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

    let (ab, ba) = currency_rates;
    let row1_value = parse_currency_input(&currency_inputs[0]);
    let row2_value = parse_currency_input(&currency_inputs[1]);
    let row1_converted = row1_value.map(|v| v * ab.rate);
    let row2_converted = row2_value.map(|v| v * ba.rate);

    let row1_input_style = if active_currency_input == 0 {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let row2_input_style = if active_currency_input == 1 {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    let currency_lines = vec![
        Line::from(vec![
            Span::styled(format!("[{}]", currency_inputs[0]), row1_input_style),
            Span::raw(format!(" {} => {}", ab.base, render_currency_value(row1_converted, &ab.quote))),
        ]),
        Line::from(vec![
            Span::styled(format!("[{}]", currency_inputs[1]), row2_input_style),
            Span::raw(format!(" {} => {}", ba.base, render_currency_value(row2_converted, &ba.quote))),
        ]),
    ];

    let currency_panel = Paragraph::new(currency_lines).block(
        Block::default()
            .title(format!(
                " Currency ({}/{}) ",
                currency_status_icon(&ab.base, &ab.status),
                currency_status_icon(&ba.base, &ba.status)
            ))
            .borders(Borders::ALL),
    );
    frame.render_widget(currency_panel, left_chunks[3]);

    // Right panel: split into weather (top), github (middle), and main content (bottom)
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
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

    // GitHub activity emoji row
    let gh_block = Block::default()
        .title(format!(" GitHub ({}) ", github_activity.status))
        .borders(Borders::ALL);
    let gh_inner_width = gh_block.inner(right_chunks[1]).width as usize;
    let max_days = (gh_inner_width + 1) / 3; // each emoji ~2 cols + 1 space
    let emoji_str: String = github_activity
        .days
        .iter()
        .take(max_days)
        .map(|(_, count)| GitHubActivity::emoji_for_count(*count))
        .collect::<Vec<&str>>()
        .join(" ");
    let gh_para = Paragraph::new(emoji_str).block(gh_block);
    frame.render_widget(gh_para, right_chunks[1]);

    // Main content
    let block = Block::default()
        .title(" Tangential Cold TUI ")
        .borders(Borders::ALL);

    let paragraph = Paragraph::new("Welcome! Press 'q' to quit.")
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, right_chunks[2]);

    // Bottom menu bar
    let cfg_style = match cfg_source {
        ConfigSource::File(_) => Style::default().fg(Color::Green),
        ConfigSource::Default(_) => Style::default().fg(Color::Yellow),
    };
    let menu = Line::from(vec![
        Span::styled(" q", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(" Quit  "),
        Span::styled("r", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(" Refresh  "),
        Span::styled(cfg_source.label(), cfg_style),
    ]);
    let menu_bar = Paragraph::new(menu)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(menu_bar, chunks[2]);
}

fn currency_units(cfg: &Config) -> (String, String) {
    let mut normalized: Vec<String> = cfg
        .currency
        .units
        .iter()
        .map(|u| u.trim().to_ascii_uppercase())
        .filter(|u| !u.is_empty())
        .collect();
    normalized.dedup();
    if normalized.len() >= 2 {
        (normalized[0].clone(), normalized[1].clone())
    } else {
        ("USD".to_string(), "EUR".to_string())
    }
}

fn parse_currency_input(raw: &str) -> Option<f64> {
    if raw.is_empty() {
        None
    } else {
        raw.parse::<f64>().ok()
    }
}

fn render_currency_value(value: Option<f64>, unit: &str) -> String {
    match value {
        Some(v) => format!("{:.4} {}", v, unit),
        None => format!("... {}", unit),
    }
}

fn currency_status_icon(currency: &str, status: &str) -> &'static str {
    if status == "OK" {
        currency_emoji(currency)
    } else {
        "⚠️"
    }
}

fn currency_emoji(currency: &str) -> &'static str {
    match currency {
        "USD" => "🇺🇸",
        "EUR" => "🇪🇺",
        "GBP" => "🇬🇧",
        "JPY" => "🇯🇵",
        "CAD" => "🇨🇦",
        "AUD" => "🇦🇺",
        "CHF" => "🇨🇭",
        _ => "💱",
    }
}

fn weekday_name_spanish(w: Weekday) -> &'static str {
    match w {
        Weekday::Mon => "Lunes",
        Weekday::Tue => "Martes",
        Weekday::Wed => "Miércoles",
        Weekday::Thu => "Jueves",
        Weekday::Fri => "Viernes",
        Weekday::Sat => "Sábado",
        Weekday::Sun => "Domingo",
    }
}

/// Full weekday (English or Spanish) plus date in Madrid time. Language toggles each UTC minute.
fn format_header_day_date(now: DateTime<Utc>) -> String {
    let local = now.with_timezone(&Madrid);
    let use_spanish = (now.timestamp() / 60) % 2 != 0;
    let weekday = if use_spanish {
        weekday_name_spanish(local.weekday()).to_string()
    } else {
        local.format("%A").to_string()
    };
    let date = local.format("%Y-%m-%d");
    format!("{weekday} {date}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_statuses(entries: &[(&str, &str)]) -> Arc<Mutex<Vec<(String, String)>>> {
        Arc::new(Mutex::new(
            entries.iter().map(|(c, u)| (c.to_string(), u.to_string())).collect(),
        ))
    }

    #[test]
    fn reset_statuses_sets_all_codes_to_pending() {
        let statuses = make_statuses(&[
            ("200", "https://example.com"),
            ("404", "https://missing.example.com"),
            ("ERR", "https://broken.example.com"),
        ]);

        reset_statuses(&statuses);

        let locked = statuses.lock().unwrap();
        for (code, _) in locked.iter() {
            assert_eq!(code, "...", "Expected '...' but got '{code}'");
        }
    }

    #[test]
    fn reset_statuses_preserves_urls() {
        let urls = vec!["https://example.com", "https://other.example.com"];
        let statuses = make_statuses(&[("200", urls[0]), ("500", urls[1])]);

        reset_statuses(&statuses);

        let locked = statuses.lock().unwrap();
        let stored_urls: Vec<&str> = locked.iter().map(|(_, u)| u.as_str()).collect();
        assert_eq!(stored_urls, urls);
    }

    #[test]
    fn reset_statuses_on_empty_list_is_noop() {
        let statuses = make_statuses(&[]);
        reset_statuses(&statuses);
        let locked = statuses.lock().unwrap();
        assert!(locked.is_empty());
    }

    #[test]
    fn reset_statuses_idempotent() {
        let statuses = make_statuses(&[("200", "https://example.com")]);
        reset_statuses(&statuses);
        reset_statuses(&statuses);
        let locked = statuses.lock().unwrap();
        assert_eq!(locked[0].0, "...");
    }

    #[test]
    fn spanish_weekday_names() {
        assert_eq!(weekday_name_spanish(Weekday::Mon), "Lunes");
        assert_eq!(weekday_name_spanish(Weekday::Wed), "Miércoles");
        assert_eq!(weekday_name_spanish(Weekday::Sun), "Domingo");
    }

    #[test]
    fn header_day_date_includes_date_and_weekday_word() {
        let t = NaiveDate::from_ymd_opt(2024, 6, 15)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc();
        let s = format_header_day_date(t);
        assert!(s.contains("2024-06-15"));
        assert!(s.contains("Saturday") || s.contains("Sábado"));
    }

    #[test]
    fn header_day_date_toggles_weekday_language_each_utc_minute() {
        let t0 = NaiveDate::from_ymd_opt(2024, 6, 15)
            .unwrap()
            .and_hms_opt(14, 30, 0)
            .unwrap()
            .and_utc();
        let t1 = t0 + chrono::Duration::minutes(1);
        let w0 = format_header_day_date(t0)
            .split_whitespace()
            .next()
            .unwrap()
            .to_string();
        let w1 = format_header_day_date(t1)
            .split_whitespace()
            .next()
            .unwrap()
            .to_string();
        assert_ne!(
            w0, w1,
            "adjacent UTC minutes should alternate English/Spanish weekday"
        );
    }
}
