use ratatui::{prelude::*, widgets::{Block, Borders, Cell, Row, Table}};
use sysinfo::System;

pub struct SysSnapshot {
    pub cpu_count: usize,
    pub total_ram: f64,
    pub used_ram: f64,
    pub cpu_load: f32,
}

pub fn take_snapshot(sys: &System) -> SysSnapshot {
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

pub fn current_cpu_load(sys: &System) -> f32 {
    if sys.cpus().is_empty() {
        0.0
    } else {
        sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / sys.cpus().len() as f32
    }
}

pub fn render_system_table(frame: &mut Frame, area: Rect, sys: &SysSnapshot) {
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

pub const SYSTEM_TABLE_HEIGHT: u16 = 4 + 2; // 4 data rows + 2 border rows

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_cpu_count_matches_sysinfo() {
        let mut sys = System::new_all();
        sys.refresh_cpu_usage();
        let snap = take_snapshot(&sys);
        assert_eq!(snap.cpu_count, sys.cpus().len());
    }

    #[test]
    fn snapshot_total_ram_positive() {
        let mut sys = System::new_all();
        sys.refresh_memory();
        let snap = take_snapshot(&sys);
        assert!(snap.total_ram > 0.0);
    }

    #[test]
    fn snapshot_used_ram_lte_total() {
        let mut sys = System::new_all();
        sys.refresh_memory();
        let snap = take_snapshot(&sys);
        assert!(snap.used_ram <= snap.total_ram);
    }

    #[test]
    fn current_cpu_load_in_range() {
        let mut sys = System::new_all();
        sys.refresh_cpu_usage();
        let load = current_cpu_load(&sys);
        assert!((0.0..=100.0).contains(&load));
    }
}
