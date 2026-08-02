use std::time::Duration;

use chrono::{DateTime, Utc};
use chrono_tz::{America::Chicago, Europe::Madrid};
use eframe::egui::{self, Color32, RichText};

use tc_core::config::ConfigSource;
use tc_core::format::{
    currency_status_icon, format_header_day_date, parse_currency_input, render_currency_value,
};
use tc_core::github::GitHubActivity;
use tc_core::{App, Snapshot};

const CYAN: Color32 = Color32::from_rgb(76, 204, 217);
const GREEN: Color32 = Color32::from_rgb(76, 204, 89);
const YELLOW: Color32 = Color32::from_rgb(230, 204, 64);
const RED: Color32 = Color32::from_rgb(230, 76, 76);
const GRAY: Color32 = Color32::from_rgb(140, 140, 140);

/// How often the UI polls `tc-core` for fresh data.
const TICK_MS: u64 = 250;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Tangential Cold — egui",
        options,
        Box::new(|cc| Ok(Box::new(TcEgui::new(cc)))),
    )
}

struct TcEgui {
    app: App,
    currency_inputs: [String; 2],
}

impl TcEgui {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (cfg, cfg_source) = tc_core::config::load();
        let mut app = App::new(cfg, cfg_source);
        app.spawn_fetchers();
        TcEgui {
            app,
            currency_inputs: [String::from("1"), String::from("1")],
        }
    }
}

impl eframe::App for TcEgui {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // egui only repaints on input by default; keep polling core on an interval.
        ui.ctx().request_repaint_after(Duration::from_millis(TICK_MS));

        let snap = self.app.snapshot();

        egui::Panel::top("header").show(ui, |ui| self.header(ui, &snap));
        egui::Panel::bottom("footer").show(ui, |ui| self.footer(ui));

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.columns(2, |cols| {
                    self.left_column(&mut cols[0], &snap);
                    right_column(&mut cols[1], &snap);
                });
            });
        });
    }
}

impl TcEgui {
    fn header(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        let now: DateTime<Utc> = Utc::now();
        let spain = now.with_timezone(&Madrid).format("%H:%M").to_string();
        let central = now.with_timezone(&Chicago).format("%H:%M").to_string();
        let day_date = format_header_day_date(now);
        let hostname = hostname::get()
            .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
            .to_string_lossy()
            .to_string();
        let lock = if snap.vpn { "🔒" } else { "🔓" };

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.colored_label(CYAN, format!("🇪🇸 {}  │  🇺🇸 {}", spain, central));
            ui.separator();
            ui.colored_label(CYAN, format!("({}) {} {}", snap.ip_city, lock, hostname));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.colored_label(CYAN, day_date);
            });
        });
        ui.add_space(4.0);
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        let (cfg_color, cfg_label) = match &self.app.cfg_source {
            ConfigSource::File(_) => (GREEN, self.app.cfg_source.label()),
            ConfigSource::Default(_) => (YELLOW, self.app.cfg_source.label()),
        };
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.app.refresh_all();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.colored_label(cfg_color, cfg_label);
            });
        });
        ui.add_space(2.0);
    }

    fn left_column(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        panel(ui, "URL Status", |ui| {
            for (code, url) in &snap.statuses {
                ui.horizontal(|ui| {
                    ui.colored_label(status_color(code), RichText::new(code.as_str()).monospace());
                    ui.label(url.as_str());
                });
            }
        });

        panel(ui, "System", |ui| {
            let s = &snap.sys;
            let ram_pct = if s.total_ram > 0.0 { (s.used_ram / s.total_ram) as f32 } else { 0.0 };
            let ram_color = level_color(ram_pct, 0.5, 0.8);
            let cpu_color = level_color(s.cpu_load, 33.3, 66.6);

            kv(ui, "CPU Count", |ui| {
                ui.label(format!("{}", s.cpu_count));
            });
            kv(ui, "RAM Total", |ui| {
                ui.label(format!("{:.1} GB", s.total_ram));
            });
            kv(ui, "RAM Usage", |ui| {
                ui.colored_label(ram_color, format!("{:.1}", s.used_ram));
                ui.label(format!("/ {:.1} GB", s.total_ram));
            });
            kv(ui, "CPU Load", |ui| {
                ui.colored_label(cpu_color, format!("{:.1}%", s.cpu_load));
            });
        });

        panel(ui, "CPU History", |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 1.0;
                if snap.cpu_history.is_empty() {
                    ui.colored_label(GRAY, "collecting…");
                }
                for &load in &snap.cpu_history {
                    ui.colored_label(
                        level_color(load, 33.3, 66.6),
                        RichText::new(spark_char(load).to_string()).monospace().size(18.0),
                    );
                }
            });
        });

        let (ab, ba) = snap.currency_rates.clone();
        let title = format!(
            "Currency ({}/{})",
            currency_status_icon(&ab.base, &ab.status),
            currency_status_icon(&ba.base, &ba.status),
        );
        panel(ui, &title, |ui| {
            currency_row(ui, &mut self.currency_inputs[0], &ab.base, ab.rate, &ab.quote);
            currency_row(ui, &mut self.currency_inputs[1], &ba.base, ba.rate, &ba.quote);
        });
    }
}

fn right_column(ui: &mut egui::Ui, snap: &Snapshot) {
    panel(ui, "Weather", |ui| {
        if snap.weather.is_empty() {
            ui.colored_label(GRAY, "No locations configured");
        } else {
            for w in &snap.weather {
                ui.monospace(format!(
                    "{:<12} {:.1}°F ({:.1}°C)   H:{:.1}°F  L:{:.1}°F   {} {}",
                    w.city, w.current_f, w.current_c, w.high_f, w.low_f, w.emoji, w.description,
                ));
            }
        }
    });

    panel(ui, &format!("GitHub ({})", snap.github.status), |ui| {
        let emojis: String = snap
            .github
            .days
            .iter()
            .take(60)
            .map(|(_, count)| GitHubActivity::emoji_for_count(*count))
            .collect::<Vec<&str>>()
            .join(" ");
        ui.label(if emojis.is_empty() { "...".to_string() } else { emojis });
    });

    panel(ui, "Tangential Cold", |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.colored_label(CYAN, RichText::new("Tangential Cold").size(22.0).strong());
            ui.add_space(20.0);
        });
    });
}

/// A single two-way currency converter row backed by an editable amount field.
fn currency_row(ui: &mut egui::Ui, input: &mut String, base: &str, rate: f64, quote: &str) {
    ui.horizontal(|ui| {
        let resp = ui.add(egui::TextEdit::singleline(input).desired_width(90.0));
        if resp.changed() {
            input.retain(|c| c.is_ascii_digit() || c == '.');
        }
        let converted = parse_currency_input(input).map(|v| v * rate);
        ui.label(format!("{} → {}", base, render_currency_value(converted, quote)));
    });
}

/// A bordered, titled section that fills the column width.
fn panel(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let width = ui.available_width();
    ui.group(|ui| {
        ui.set_min_width(width - 16.0);
        ui.vertical(|ui| {
            ui.colored_label(CYAN, RichText::new(title).strong().size(16.0));
            ui.separator();
            add(ui);
        });
    });
    ui.add_space(8.0);
}

/// A labeled row: a fixed-width monospace key followed by caller-drawn value widgets.
fn kv(ui: &mut egui::Ui, key: &str, value: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.monospace(format!("{:<10}", key));
        value(ui);
    });
}

fn status_color(code: &str) -> Color32 {
    match code {
        "200" => GREEN,
        c if c.starts_with('3') => YELLOW,
        c if c.starts_with('4') || c.starts_with('5') => RED,
        "..." => GRAY,
        _ => RED,
    }
}

/// Green below `warn`, yellow below `danger`, red at or above `danger`.
fn level_color(value: f32, warn: f32, danger: f32) -> Color32 {
    if value >= danger {
        RED
    } else if value >= warn {
        YELLOW
    } else {
        GREEN
    }
}

fn spark_char(load: f32) -> char {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let idx = ((load / 100.0) * (BARS.len() as f32 - 1.0))
        .round()
        .clamp(0.0, (BARS.len() - 1) as f32) as usize;
    BARS[idx]
}
