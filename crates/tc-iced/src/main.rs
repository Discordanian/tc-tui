use std::time::Duration;

use chrono::{DateTime, Utc};
use chrono_tz::{America::Chicago, Europe::Madrid};
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::widget::text::Shaping;
use iced::{time, Alignment, Border, Color, Element, Length, Subscription, Task};

use tc_core::config::ConfigSource;
use tc_core::format::{
    currency_status_icon, format_header_day_date, parse_currency_input, render_currency_value,
};
use tc_core::github::GitHubActivity;
use tc_core::{App, Snapshot};

const CYAN: Color = Color { r: 0.30, g: 0.80, b: 0.85, a: 1.0 };
const GREEN: Color = Color { r: 0.30, g: 0.80, b: 0.35, a: 1.0 };
const YELLOW: Color = Color { r: 0.90, g: 0.80, b: 0.25, a: 1.0 };
const RED: Color = Color { r: 0.90, g: 0.30, b: 0.30, a: 1.0 };
const GRAY: Color = Color { r: 0.55, g: 0.55, b: 0.55, a: 1.0 };
const BORDER: Color = Color { r: 0.30, g: 0.30, b: 0.35, a: 1.0 };

/// How often the UI polls `tc-core` for fresh data.
const TICK_MS: u64 = 250;

fn main() -> iced::Result {
    iced::application("Tangential Cold — iced", TcIced::update, TcIced::view)
        .subscription(TcIced::subscription)
        // Shared monochrome emoji font from tc-core; cosmic-text uses it as a
        // glyph fallback once Advanced shaping is enabled on a text widget.
        .font(tc_core::assets::NOTO_EMOJI_TTF)
        .run_with(TcIced::new)
}

struct TcIced {
    app: App,
    snapshot: Snapshot,
    currency_inputs: [String; 2],
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    Refresh,
    CurrencyInput(usize, String),
}

impl TcIced {
    fn new() -> (Self, Task<Message>) {
        let (cfg, cfg_source) = tc_core::config::load();
        let mut app = App::new(cfg, cfg_source);
        app.spawn_fetchers();
        let snapshot = app.snapshot();
        (
            TcIced {
                app,
                snapshot,
                currency_inputs: [String::from("1"), String::from("1")],
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => self.snapshot = self.app.snapshot(),
            Message::Refresh => self.app.refresh_all(),
            Message::CurrencyInput(idx, value) => {
                if idx < self.currency_inputs.len() {
                    self.currency_inputs[idx] =
                        value.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
                }
            }
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        time::every(Duration::from_millis(TICK_MS)).map(|_| Message::Tick)
    }

    fn view(&self) -> Element<'_, Message> {
        let left = column![
            self.url_status(),
            self.system_info(),
            self.cpu_history(),
            self.currency(),
        ]
        .spacing(10)
        .width(Length::FillPortion(1));

        let right = column![self.weather(), self.github(), welcome()]
            .spacing(10)
            .width(Length::FillPortion(2));

        let body = row![left, right].spacing(10);

        column![
            self.header(),
            scrollable(container(body).padding(10)).height(Length::Fill),
            self.footer(),
        ]
        .into()
    }

    fn header(&self) -> Element<'_, Message> {
        let now: DateTime<Utc> = Utc::now();
        let spain = now.with_timezone(&Madrid).format("%H:%M").to_string();
        let central = now.with_timezone(&Chicago).format("%H:%M").to_string();
        let day_date = format_header_day_date(now);
        let hostname = hostname::get()
            .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
            .to_string_lossy()
            .to_string();
        let lock = if self.snapshot.vpn { "🔒" } else { "🔓" };
        let center = format!("({}) {} {}", self.snapshot.ip_city, lock, hostname);

        row![
            emoji_text(format!("🇪🇸 {}  │  🇺🇸 {}", spain, central)).color(CYAN),
            Space::with_width(Length::Fill),
            emoji_text(center).color(CYAN),
            Space::with_width(Length::Fill),
            text(day_date).color(CYAN),
        ]
        .padding(10)
        .align_y(Alignment::Center)
        .into()
    }

    fn url_status(&self) -> Element<'_, Message> {
        let mut col = column![panel_title("URL Status")].spacing(4);
        for (code, url) in &self.snapshot.statuses {
            col = col.push(
                row![
                    text(code.clone()).color(status_color(code)).width(Length::Fixed(48.0)),
                    text(url.clone()),
                ]
                .spacing(8),
            );
        }
        panel(col.into())
    }

    fn system_info(&self) -> Element<'_, Message> {
        let s = &self.snapshot.sys;
        let ram_pct = if s.total_ram > 0.0 { (s.used_ram / s.total_ram) as f32 } else { 0.0 };
        let ram_color = level_color(ram_pct, 0.5, 0.8);
        let cpu_color = level_color(s.cpu_load, 33.3, 66.6);

        let col = column![
            panel_title("System"),
            labeled("CPU Count", text(format!("{}", s.cpu_count)).into()),
            labeled("RAM Total", text(format!("{:.1} GB", s.total_ram)).into()),
            labeled(
                "RAM Usage",
                row![
                    text(format!("{:.1}", s.used_ram)).color(ram_color),
                    text(format!(" / {:.1} GB", s.total_ram)),
                ]
                .into(),
            ),
            labeled("CPU Load", text(format!("{:.1}%", s.cpu_load)).color(cpu_color).into()),
        ]
        .spacing(4);
        panel(col.into())
    }

    fn cpu_history(&self) -> Element<'_, Message> {
        let mut bars = row![].spacing(1);
        for &load in &self.snapshot.cpu_history {
            bars = bars.push(
                text(spark_char(load).to_string())
                    .color(level_color(load, 33.3, 66.6))
                    .size(18.0),
            );
        }
        panel(column![panel_title("CPU History"), bars].spacing(4).into())
    }

    fn currency(&self) -> Element<'_, Message> {
        let (ab, ba) = &self.snapshot.currency_rates;
        let v0 = parse_currency_input(&self.currency_inputs[0]).map(|v| v * ab.rate);
        let v1 = parse_currency_input(&self.currency_inputs[1]).map(|v| v * ba.rate);

        let title = format!(
            "Currency ({}/{})",
            currency_status_icon(&ab.base, &ab.status),
            currency_status_icon(&ba.base, &ba.status),
        );

        let row0 = row![
            text_input("amount", &self.currency_inputs[0])
                .on_input(|s| Message::CurrencyInput(0, s))
                .width(Length::Fixed(110.0)),
            text(format!(" {} → {}", ab.base, render_currency_value(v0, &ab.quote))),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let row1 = row![
            text_input("amount", &self.currency_inputs[1])
                .on_input(|s| Message::CurrencyInput(1, s))
                .width(Length::Fixed(110.0)),
            text(format!(" {} → {}", ba.base, render_currency_value(v1, &ba.quote))),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        panel(column![emoji_text(title).size(18.0).color(CYAN), row0, row1].spacing(6).into())
    }

    fn weather(&self) -> Element<'_, Message> {
        let mut col = column![panel_title("Weather")].spacing(4);
        if self.snapshot.weather.is_empty() {
            col = col.push(text("No locations configured").color(GRAY));
        } else {
            for w in &self.snapshot.weather {
                col = col.push(emoji_text(format!(
                    "{:<12} {:.1}°F ({:.1}°C)   H:{:.1}°F  L:{:.1}°F   {} {}",
                    w.city, w.current_f, w.current_c, w.high_f, w.low_f, w.emoji, w.description,
                )));
            }
        }
        panel(col.into())
    }

    fn github(&self) -> Element<'_, Message> {
        let emojis: String = self
            .snapshot
            .github
            .days
            .iter()
            .take(60)
            .map(|(_, count)| GitHubActivity::emoji_for_count(*count))
            .collect::<Vec<&str>>()
            .join(" ");
        let body = if emojis.is_empty() { "...".to_string() } else { emojis };
        panel(
            column![
                text(format!("GitHub ({})", self.snapshot.github.status))
                    .size(18.0)
                    .color(CYAN),
                emoji_text(body),
            ]
            .spacing(4)
            .into(),
        )
    }

    fn footer(&self) -> Element<'_, Message> {
        let cfg_color = match self.app.cfg_source {
            ConfigSource::File(_) => GREEN,
            ConfigSource::Default(_) => YELLOW,
        };
        row![
            button(text("Refresh")).on_press(Message::Refresh),
            Space::with_width(Length::Fill),
            text(self.app.cfg_source.label()).color(cfg_color),
        ]
        .padding(10)
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }
}

fn welcome() -> Element<'static, Message> {
    panel(
        container(text("Tangential Cold").size(22.0).color(CYAN))
            .center_x(Length::Fill)
            .padding(20)
            .into(),
    )
}

fn panel_title(title: &str) -> Element<'_, Message> {
    text(title.to_string()).size(18.0).color(CYAN).into()
}

/// Text that may contain emoji / complex Unicode.
///
/// iced defaults to `Shaping::Basic`, which skips font fallback — so missing
/// glyphs stay blank even when an emoji font is loaded. Advanced shaping lets
/// cosmic-text pull glyphs from the bundled Noto Emoji (and any system emoji
/// fonts) when the primary font doesn't have them.
fn emoji_text<'a>(
    content: impl text::IntoFragment<'a>,
) -> text::Text<'a, iced::Theme, iced::Renderer> {
    text(content).shaping(Shaping::Advanced)
}

fn labeled<'a>(label: &'a str, value: Element<'a, Message>) -> Element<'a, Message> {
    row![text(label.to_string()).width(Length::Fixed(120.0)), value]
        .spacing(8)
        .into()
}

fn panel<'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    container(content)
        .padding(10)
        .width(Length::Fill)
        .style(|_theme| container::Style {
            border: Border {
                color: BORDER,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..container::Style::default()
        })
        .into()
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

fn spark_char(load: f32) -> char {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let idx = ((load / 100.0) * (BARS.len() as f32 - 1.0))
        .round()
        .clamp(0.0, (BARS.len() - 1) as f32) as usize;
    BARS[idx]
}
