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
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_emoji_font(&cc.egui_ctx);
        let (cfg, cfg_source) = tc_core::config::load();
        let mut app = App::new(cfg, cfg_source);
        app.spawn_fetchers();
        TcEgui {
            app,
            currency_inputs: [String::from("1"), String::from("1")],
        }
    }
}

/// Register the bundled monochrome emoji font as a fallback for both the
/// proportional and monospace families.
///
/// egui/epaint can't rasterize color emoji fonts, so the default fonts leave
/// many glyphs (notably the weather symbols) blank. Adding this monochrome font
/// at lowest priority fills those gaps without overriding the normal text fonts.
/// The bytes live in `tc-core` so other frontends can reuse the same asset.
fn install_emoji_font(ctx: &egui::Context) {
    use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};

    ctx.add_font(FontInsert::new(
        "noto-emoji",
        egui::FontData::from_static(tc_core::assets::NOTO_EMOJI_TTF),
        vec![
            InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: FontPriority::Lowest,
            },
            InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: FontPriority::Lowest,
            },
        ],
    ));
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
            ui.colored_label(CYAN, format!("🇪🇸 {}  |  🇺🇸 {}", spain, central));
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
            cpu_wave(ui, &snap.cpu_history);
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
        // Days are newest-first; keep the most recent that fit on one line and
        // drop only the legacy (oldest) days, so the latest stay fully visible.
        let font = egui::TextStyle::Body.resolve(ui.style());
        let avail = ui.available_width();
        let space_w = ui.ctx().fonts_mut(|f| f.glyph_width(&font, ' '));

        let mut kept: Vec<(&str, Color32)> = Vec::new();
        let mut used = 0.0;
        for (_, count) in &snap.github.days {
            let emoji = GitHubActivity::emoji_for_count(*count);
            let w = ui
                .ctx()
                .fonts_mut(|f| f.layout_no_wrap(emoji.to_string(), font.clone(), Color32::WHITE).size().x);
            let add = if kept.is_empty() { w } else { w + space_w };
            if !kept.is_empty() && used + add > avail {
                break;
            }
            used += add;
            kept.push((emoji, github_color(*count)));
        }

        if kept.is_empty() {
            ui.label("...");
            return;
        }
        // Tint each (monochrome) glyph by its activity level. `horizontal` keeps
        // everything on one line (no wrapping); the loop above already ensured fit.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = space_w;
            for (emoji, color) in kept {
                ui.colored_label(color, emoji);
            }
        });
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

/// Tint for a GitHub day's activity level, matching `emoji_for_count`'s tiers:
/// none (❌) red, light (✅) green, busy (🌟) yellow, heavy (🚀) cyan.
fn github_color(count: u32) -> Color32 {
    match count {
        0 => RED,
        1..=3 => GREEN,
        4..=6 => YELLOW,
        _ => CYAN,
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

/// Height of the CPU-history wave, in points.
const WAVE_HEIGHT: f32 = 48.0;
/// Smoothing factor for the exponential moving average (0 = very smooth, 1 = raw).
const WAVE_EMA_ALPHA: f32 = 0.4;
/// Catmull-Rom subdivisions per sample gap (higher = smoother curve).
const WAVE_SUBDIVISIONS: usize = 8;

/// Draw the CPU-load history as a smoothed, filled wave.
///
/// The samples are softened with an exponential moving average, interpolated
/// with a Catmull-Rom spline for a flowing curve, then rendered as a translucent
/// filled area with the line on top. The color follows the latest load
/// (green/yellow/red).
fn cpu_wave(ui: &mut egui::Ui, history: &[f32]) {
    let (rect, _resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), WAVE_HEIGHT), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // Faint baseline along the bottom.
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        egui::Stroke::new(1.0, Color32::from_gray(70)),
    );

    if history.len() < 2 {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "collecting…",
            egui::FontId::proportional(12.0),
            GRAY,
        );
        return;
    }

    let smoothed = ema(history, WAVE_EMA_ALPHA);
    let curve = catmull_rom(&smoothed, WAVE_SUBDIVISIONS);

    let to_pos = |x_frac: f32, val: f32| {
        egui::pos2(
            rect.left() + rect.width() * x_frac,
            rect.bottom() - rect.height() * (val / 100.0).clamp(0.0, 1.0),
        )
    };
    let line: Vec<egui::Pos2> = curve.iter().map(|&(x, v)| to_pos(x, v)).collect();

    let color = level_color(*history.last().unwrap(), 33.3, 66.6);
    let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 48);

    // Fill the area under the curve as a triangle strip down to the baseline.
    // Built by hand (rather than a closed PathShape) so concave curves fill cleanly.
    let mut mesh = egui::Mesh::default();
    for seg in line.windows(2) {
        let (p0, p1) = (seg[0], seg[1]);
        let base = mesh.vertices.len() as u32;
        for p in [p0, p1, egui::pos2(p1.x, rect.bottom()), egui::pos2(p0.x, rect.bottom())] {
            mesh.colored_vertex(p, fill);
        }
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    painter.add(egui::Shape::mesh(mesh));

    painter.add(egui::Shape::line(line, egui::Stroke::new(1.5, color)));
}

/// Exponential moving average, softening spikes before plotting.
fn ema(data: &[f32], alpha: f32) -> Vec<f32> {
    let mut out = Vec::with_capacity(data.len());
    let mut prev = data.first().copied().unwrap_or(0.0);
    for &v in data {
        prev = alpha * v + (1.0 - alpha) * prev;
        out.push(prev);
    }
    out
}

/// Catmull-Rom spline through equally-spaced sample values.
///
/// Returns `(x_fraction, value)` points, where `x_fraction` spans `0.0..=1.0`
/// across the samples, subdividing each gap for a smooth curve.
fn catmull_rom(values: &[f32], subdivisions: usize) -> Vec<(f32, f32)> {
    let n = values.len();
    if n < 2 {
        return values.iter().map(|&v| (0.0, v)).collect();
    }
    let denom = (n - 1) as f32;
    let at = |i: isize| values[i.clamp(0, n as isize - 1) as usize];

    let mut out = Vec::with_capacity((n - 1) * subdivisions + 1);
    for i in 0..(n - 1) {
        let (p0, p1, p2, p3) = (
            at(i as isize - 1),
            at(i as isize),
            at(i as isize + 1),
            at(i as isize + 2),
        );
        for s in 0..subdivisions {
            let t = s as f32 / subdivisions as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            let v = 0.5
                * (2.0 * p1
                    + (-p0 + p2) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                    + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3);
            out.push(((i as f32 + t) / denom, v));
        }
    }
    out.push((1.0, values[n - 1]));
    out
}
