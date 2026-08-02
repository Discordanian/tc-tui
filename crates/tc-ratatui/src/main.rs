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
use std::time::Duration;

use tc_core::config::ConfigSource;
use tc_core::format::{
    currency_status_icon, format_header_day_date, parse_currency_input, render_currency_value,
};
use tc_core::github::GitHubActivity;
use tc_core::system::SysSnapshot;
use tc_core::{App, Snapshot};

const BAR_GRAPH_HEIGHT: u16 = 3;
const CURRENCY_BOX_HEIGHT: u16 = 4;
const SYSTEM_TABLE_HEIGHT: u16 = 4 + 2; // 4 data rows + 2 border rows

type Terminal = ratatui::Terminal<CrosstermBackend<Stdout>>;

fn main() -> io::Result<()> {
    let (cfg, cfg_source) = tc_core::config::load();
    let mut app = App::new(cfg, cfg_source);
    app.spawn_fetchers();

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal, app: &App) -> io::Result<()> {
    let cpu_history_len = app.cfg.display.cpu_history_len;
    let mut currency_inputs = [String::from("1"), String::from("1")];
    let mut active_currency_input = 0usize;

    loop {
        let snap = app.snapshot();

        terminal.draw(|frame| {
            ui(
                frame,
                &snap,
                &currency_inputs,
                active_currency_input,
                cpu_history_len,
                &app.cfg_source,
            )
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('r') => app.refresh_all(),
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

fn render_system_table(frame: &mut Frame, area: Rect, sys: &SysSnapshot) {
    let pct = if sys.total_ram > 0.0 { sys.used_ram / sys.total_ram } else { 0.0 };
    let ram_color = if pct >= 0.80 {
        Color::Red
    } else if pct >= 0.50 {
        Color::Yellow
    } else {
        Color::Green
    };

    let rows = vec![
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
            Cell::from(Line::from(vec![
                Span::styled(format!("{:.1}", sys.used_ram), Style::default().fg(ram_color)),
                Span::raw(format!(" / {:.1} GB", sys.total_ram)),
            ])),
        ]),
        Row::new(vec![
            Cell::from("CPU Load"),
            {
                let cpu_color = if sys.cpu_load > 66.6 {
                    Color::Red
                } else if sys.cpu_load > 33.3 {
                    Color::Yellow
                } else {
                    Color::Green
                };
                Cell::from(Span::styled(
                    format!("{:.1}%", sys.cpu_load),
                    Style::default().fg(cpu_color),
                ))
            },
        ]),
    ];

    let table = Table::new(rows, [Constraint::Length(12), Constraint::Min(0)])
        .block(Block::default().title(" System ").borders(Borders::ALL))
        .style(Style::default().fg(Color::Cyan));

    frame.render_widget(table, area);
}

fn ui(
    frame: &mut Frame,
    snap: &Snapshot,
    currency_inputs: &[String; 2],
    active_currency_input: usize,
    cpu_history_len: usize,
    cfg_source: &ConfigSource,
) {
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
            Constraint::Length(snap.statuses.len() as u16 + 3),
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
    let lock = if snap.vpn { "🔒" } else { "🔓" };
    // "(city) 🔒 hostname" — emoji is 2 cols, city parens + space + space = city.len()+4
    let center_text = format!("({}) {} {}", snap.ip_city, lock, hostname.to_string_lossy());
    let center_width = (snap.ip_city.len() + 2 + 1 + 2 + 1 + hostname.len()) as u16 + 2;

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
    let rows: Vec<Row> = snap
        .statuses
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

    render_system_table(frame, left_chunks[1], &snap.sys);

    // Left panel: CPU bar graph
    let graph_block = Block::default()
        .title(" CPU History ")
        .borders(Borders::ALL);
    let graph_inner = graph_block.inner(left_chunks[2]);
    frame.render_widget(graph_block, left_chunks[2]);

    let w = graph_inner.width as usize;
    let bar_width = (w / cpu_history_len.max(1)).max(1);
    let gap = if bar_width > 1 { 1 } else { 0 };
    let fill = bar_width.saturating_sub(gap);
    let buf = frame.buffer_mut();

    for (i, &load) in snap.cpu_history.iter().enumerate() {
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

    let (ab, ba) = &snap.currency_rates;
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

    // Right panel: weather (optional), github, main content
    let show_weather = !snap.weather.is_empty();
    let mut right_constraints: Vec<Constraint> = Vec::new();
    if show_weather {
        right_constraints.push(Constraint::Length(snap.weather.len() as u16 + 2));
    }
    right_constraints.push(Constraint::Length(3));
    right_constraints.push(Constraint::Min(0));

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(right_constraints)
        .split(body_chunks[1]);

    let mut right_idx = 0usize;

    // Weather box (only rendered when locations are configured)
    if show_weather {
        let weather_lines: Vec<Line> = snap.weather.iter().map(|w| {
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
        frame.render_widget(weather_para, right_chunks[right_idx]);
        right_idx += 1;
    }

    // GitHub activity emoji row
    let gh_block = Block::default()
        .title(format!(" GitHub ({}) ", snap.github.status))
        .borders(Borders::ALL);
    let gh_inner_width = gh_block.inner(right_chunks[right_idx]).width as usize;
    let max_days = (gh_inner_width + 1) / 3; // each emoji ~2 cols + 1 space
    let emoji_str: String = snap
        .github
        .days
        .iter()
        .take(max_days)
        .map(|(_, count)| GitHubActivity::emoji_for_count(*count))
        .collect::<Vec<&str>>()
        .join(" ");
    let gh_para = Paragraph::new(emoji_str).block(gh_block);
    frame.render_widget(gh_para, right_chunks[right_idx]);
    right_idx += 1;

    // Main content
    let block = Block::default()
        .title(" Tangential Cold TUI ")
        .borders(Borders::ALL);

    let paragraph = Paragraph::new("Welcome! Press 'q' to quit.")
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, right_chunks[right_idx]);

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
