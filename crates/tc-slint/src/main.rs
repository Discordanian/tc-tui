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

    // CPU history bars
    let bars: Vec<Bar> = snap
        .cpu_history
        .iter()
        .map(|&load| Bar {
            level: (load / 100.0).clamp(0.0, 1.0),
            color: level_color(load, 33.3, 66.6),
        })
        .collect();
    ui.set_cpu_bars(ModelRc::from(bars.as_slice()));

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

    // GitHub
    ui.set_github_title(format!("GitHub ({})", snap.github.status).into());
    let emojis: String = snap
        .github
        .days
        .iter()
        .take(60)
        .map(|(_, count)| GitHubActivity::emoji_for_count(*count))
        .collect::<Vec<&str>>()
        .join(" ");
    let github_emojis: SharedString = if emojis.is_empty() { "...".into() } else { emojis.into() };
    ui.set_github_emojis(github_emojis);

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

fn status_color(code: &str) -> Color {
    match code {
        "200" => GREEN,
        c if c.starts_with('3') => YELLOW,
        c if c.starts_with('4') || c.starts_with('5') => RED,
        "..." => GRAY,
        _ => RED,
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
