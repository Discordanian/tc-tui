use std::rc::Rc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use chrono_tz::{America::Chicago, Europe::Madrid};
use slint::{Color, ComponentHandle, ModelRc, SharedString};

use tc_core::config::ConfigSource;
use tc_core::format::{
    currency_status_icon, format_header_day_date, parse_currency_input, render_currency_value,
};
use tc_core::github::GitHubActivity;
use tc_core::{App, Snapshot};

slint::include_modules!();

/// How often the UI polls `tc-core` for fresh data.
const TICK_MS: u64 = 250;

fn main() -> Result<(), slint::PlatformError> {
    let (cfg, cfg_source) = tc_core::config::load();
    let mut app = App::new(cfg, cfg_source);
    app.spawn_fetchers();
    let app = Rc::new(app);

    let ui = AppWindow::new()?;
    // Must run after the Slint platform is up (`AppWindow::new`).
    install_emoji_font();
    ui.set_currency_input_0("1".into());
    ui.set_currency_input_1("1".into());

    {
        let app = app.clone();
        ui.on_refresh_clicked(move || app.refresh_all());
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_currency_edited(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_currency_input_0(filter_amount(&ui.get_currency_input_0()).into());
                ui.set_currency_input_1(filter_amount(&ui.get_currency_input_1()).into());
            }
        });
    }

    // Poll core on an interval and push the snapshot into the UI.
    let timer = slint::Timer::default();
    {
        let ui_weak = ui.as_weak();
        let app = app.clone();
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(TICK_MS),
            move || {
                if let Some(ui) = ui_weak.upgrade() {
                    update_ui(&ui, &app.snapshot(), &app.cfg_source);
                }
            },
        );
    }

    // Populate once so the first frame isn't blank.
    update_ui(&ui, &app.snapshot(), &app.cfg_source);

    ui.run()
}

fn update_ui(ui: &AppWindow, snap: &Snapshot, cfg_source: &ConfigSource) {
    // Header
    let now: DateTime<Utc> = Utc::now();
    let spain = now.with_timezone(&Madrid).format("%H:%M").to_string();
    let central = now.with_timezone(&Chicago).format("%H:%M").to_string();
    ui.set_times_text(format!("🇪🇸 {}  │  🇺🇸 {}", spain, central).into());

    let hostname = hostname::get()
        .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
        .to_string_lossy()
        .to_string();
    let lock = if snap.vpn { "🔒" } else { "🔓" };
    ui.set_center_text(format!("({}) {} {}", snap.ip_city, lock, hostname).into());
    ui.set_date_text(format_header_day_date(now).into());

    // URL status
    let rows: Vec<UrlRow> = snap
        .statuses
        .iter()
        .map(|(code, url)| UrlRow {
            code: code.as_str().into(),
            code_color: status_color(code),
            url: url.as_str().into(),
        })
        .collect();
    ui.set_url_rows(ModelRc::from(rows.as_slice()));

    // System
    let s = &snap.sys;
    ui.set_cpu_count(format!("{}", s.cpu_count).into());
    ui.set_ram_total(format!("{:.1} GB", s.total_ram).into());
    ui.set_ram_used(format!("{:.1} / {:.1} GB", s.used_ram, s.total_ram).into());
    let ram_pct = if s.total_ram > 0.0 { (s.used_ram / s.total_ram) as f32 } else { 0.0 };
    ui.set_ram_used_color(level_color(ram_pct, 0.5, 0.8));
    ui.set_cpu_load(format!("{:.1}%", s.cpu_load).into());
    ui.set_cpu_load_color(level_color(s.cpu_load, 33.3, 66.6));

    // CPU history wave (same EMA + Catmull-Rom as iced/egui)
    let wave = cpu_wave_paths(&snap.cpu_history);
    ui.set_cpu_wave_collecting(wave.collecting);
    ui.set_cpu_wave_fill(wave.fill.into());
    ui.set_cpu_wave_line(wave.line.into());
    ui.set_cpu_wave_color(wave.color);
    ui.set_cpu_wave_fill_color(wave.fill_color);

    // Currency
    let (ab, ba) = &snap.currency_rates;
    ui.set_currency_title(
        format!(
            "Currency ({}/{})",
            currency_status_icon(&ab.base, &ab.status),
            currency_status_icon(&ba.base, &ba.status),
        )
        .into(),
    );
    let in0 = ui.get_currency_input_0().to_string();
    let in1 = ui.get_currency_input_1().to_string();
    let c0 = parse_currency_input(&in0).map(|v| v * ab.rate);
    let c1 = parse_currency_input(&in1).map(|v| v * ba.rate);
    ui.set_currency_result_0(
        format!("{} → {}", ab.base, render_currency_value(c0, &ab.quote)).into(),
    );
    ui.set_currency_result_1(
        format!("{} → {}", ba.base, render_currency_value(c1, &ba.quote)).into(),
    );

    // Weather — fixed columns in the UI; emoji on its own Text/font.
    let weather_rows: Vec<WeatherRow> = if snap.weather.is_empty() {
        vec![WeatherRow {
            city: "No locations configured".into(),
            current: SharedString::default(),
            range: SharedString::default(),
            emoji: SharedString::default(),
            description: SharedString::default(),
        }]
    } else {
        snap.weather
            .iter()
            .map(|w| WeatherRow {
                city: w.city.as_str().into(),
                current: format!("{:.1}°F ({:.1}°C)", w.current_f, w.current_c).into(),
                range: format!("H:{:.1}°F  L:{:.1}°F", w.high_f, w.low_f).into(),
                emoji: w.emoji.as_str().into(),
                description: w.description.as_str().into(),
            })
            .collect()
    };
    ui.set_weather_rows(ModelRc::from(weather_rows.as_slice()));

    // GitHub — same fit logic as iced: newest-first, drop oldest that don't fit,
    // tint each day by activity level.
    ui.set_github_title(format!("GitHub ({})", snap.github.status).into());
    ui.set_github_days(ModelRc::from(github_days_for_width(snap, github_avail_width(ui)).as_slice()));

    // Footer
    ui.set_cfg_text(cfg_source.label().into());
    ui.set_cfg_color(match cfg_source {
        ConfigSource::File(_) => GREEN,
        ConfigSource::Default(_) => YELLOW,
    });
}

fn filter_amount(raw: &str) -> String {
    raw.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect()
}

const GREEN: Color = Color::from_rgb_u8(76, 204, 89);
const YELLOW: Color = Color::from_rgb_u8(230, 204, 64);
const RED: Color = Color::from_rgb_u8(230, 76, 76);
const GRAY: Color = Color::from_rgb_u8(140, 140, 140);
const CYAN: Color = Color::from_rgb_u8(76, 204, 217);

/// Approximate glyph+gap width used when fitting GitHub days (matches iced).
const GITHUB_EMOJI_W: f32 = 22.0;
const GITHUB_GAP: f32 = 4.0;

/// Height of the CPU-history wave viewbox (matches iced/egui).
const WAVE_HEIGHT: f32 = 48.0;
/// Smoothing factor for the exponential moving average (0 = very smooth, 1 = raw).
const WAVE_EMA_ALPHA: f32 = 0.4;
/// Catmull-Rom subdivisions per sample gap (higher = smoother curve).
const WAVE_SUBDIVISIONS: usize = 8;
/// Logical width of the wave viewbox (x is a percentage along history).
const WAVE_VIEW_W: f32 = 100.0;

/// Register the shared monochrome Noto Emoji font under its family name.
///
/// Same asset as egui/iced (`tc_core::assets::NOTO_EMOJI_TTF`). We only register
/// the family here — widgets that need emoji set `font-family: "Noto Emoji"`.
/// Do **not** replace `GenericFamily::Emoji` with this Latin-less font: Slint can
/// then shape whole mixed strings with it and Latin text disappears (blank weather).
fn install_emoji_font() {
    use slint::fontique_010::fontique;
    use std::sync::Arc;

    let blob = fontique::Blob::new(Arc::new(tc_core::assets::NOTO_EMOJI_TTF.to_vec()));
    let mut collection = slint::fontique_010::shared_collection();
    let _fonts = collection.register_fonts(blob, None);
}

/// Available width for the GitHub emoji row, mirroring iced's layout math:
/// body padding, right column ≈ 2/3, panel padding.
fn github_avail_width(ui: &AppWindow) -> f32 {
    let physical = ui.window().size();
    let scale = ui.window().scale_factor().max(0.1);
    let window_w = physical.width as f32 / scale;
    let content_w = (window_w - 20.0).max(0.0);
    let right_w = ((content_w - 10.0) * 2.0 / 3.0).max(0.0);
    (right_w - 20.0).max(0.0)
}

/// Newest-first days that fit in `avail`; oldest are dropped. Tinted like iced.
fn github_days_for_width(snap: &Snapshot, avail: f32) -> Vec<GhDay> {
    let mut kept: Vec<GhDay> = Vec::new();
    let mut used = 0.0;
    for (_, count) in &snap.github.days {
        let add = if kept.is_empty() {
            GITHUB_EMOJI_W
        } else {
            GITHUB_EMOJI_W + GITHUB_GAP
        };
        if !kept.is_empty() && used + add > avail {
            break;
        }
        used += add;
        kept.push(GhDay {
            emoji: GitHubActivity::emoji_for_count(*count).into(),
            color: github_color(*count),
        });
    }
    if kept.is_empty() {
        kept.push(GhDay {
            emoji: "...".into(),
            color: GRAY,
        });
    }
    kept
}

fn status_color(code: &str) -> Color {
    match code {
        "200" => GREEN,
        c if c.starts_with('3') => YELLOW,
        c if c.starts_with('4') || c.starts_with('5') => RED,
        "..." => GRAY,
        _ => RED,
    }
}

/// Tint for a GitHub day's activity level, matching iced / `emoji_for_count` tiers.
fn github_color(count: u32) -> Color {
    match count {
        0 => RED,
        1..=3 => GREEN,
        4..=6 => YELLOW,
        _ => CYAN,
    }
}

/// Green below `warn`, yellow below `danger`, red at or above `danger`.
fn level_color(value: f32, warn: f32, danger: f32) -> Color {
    if value >= danger {
        RED
    } else if value >= warn {
        YELLOW
    } else {
        GREEN
    }
}

struct CpuWavePaths {
    fill: String,
    line: String,
    color: Color,
    fill_color: Color,
    collecting: bool,
}

/// Build SVG path commands for the CPU wave in a `WAVE_VIEW_W`×`WAVE_HEIGHT` viewbox.
///
/// Same EMA + Catmull-Rom pipeline as iced/egui; Slint renders via `Path.commands`.
fn cpu_wave_paths(history: &[f32]) -> CpuWavePaths {
    let transparent = Color::from_argb_u8(0, 0, 0, 0);
    if history.len() < 2 {
        return CpuWavePaths {
            fill: String::new(),
            line: String::new(),
            color: transparent,
            fill_color: transparent,
            collecting: true,
        };
    }

    let smoothed = ema(history, WAVE_EMA_ALPHA);
    let curve = catmull_rom(&smoothed, WAVE_SUBDIVISIONS);
    let points: Vec<(f32, f32)> = curve
        .iter()
        .map(|&(x_frac, val)| {
            (
                WAVE_VIEW_W * x_frac,
                WAVE_HEIGHT - WAVE_HEIGHT * (val / 100.0).clamp(0.0, 1.0),
            )
        })
        .collect();

    let color = level_color(*history.last().unwrap(), 33.3, 66.6);
    let fill_color = Color::from_argb_u8(48, color.red(), color.green(), color.blue());

    let mut line = String::new();
    let mut fill = String::new();
    for (i, &(x, y)) in points.iter().enumerate() {
        let cmd = if i == 0 { "M" } else { "L" };
        line.push_str(&format!("{cmd} {x:.2} {y:.2} "));
        fill.push_str(&format!("{cmd} {x:.2} {y:.2} "));
    }
    let (first_x, _) = points[0];
    let (last_x, _) = *points.last().unwrap();
    fill.push_str(&format!(
        "L {last_x:.2} {WAVE_HEIGHT:.2} L {first_x:.2} {WAVE_HEIGHT:.2} Z"
    ));

    CpuWavePaths {
        fill,
        line,
        color,
        fill_color,
        collecting: false,
    }
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
