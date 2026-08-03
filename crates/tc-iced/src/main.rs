use std::time::Duration;

use chrono::{DateTime, Utc};
use chrono_tz::{America::Chicago, Europe::Madrid};
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::widget::text::Shaping;
use iced::{
    time, window, Alignment, Border, Color, Element, Font, Length, Point, Rectangle, Renderer,
    Size, Subscription, Task, Theme,
};

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
    /// Latest window size; used to fit the GitHub row without `responsive`
    /// (which fills height and panics inside a vertical `scrollable`).
    window_size: Size,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    Refresh,
    CurrencyInput(usize, String),
    WindowResized(Size),
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
                window_size: Size::new(1024.0, 768.0),
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
            Message::WindowResized(size) => self.window_size = size,
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            time::every(Duration::from_millis(TICK_MS)).map(|_| Message::Tick),
            window::resize_events().map(|(_, size)| Message::WindowResized(size)),
        ])
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
        let wave = Canvas::new(CpuWave {
            history: self.snapshot.cpu_history.clone(),
        })
        .width(Length::Fill)
        .height(WAVE_HEIGHT);

        panel(column![panel_title("CPU History"), wave].spacing(4).into())
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
                col = col.push(
                    emoji_text(format!(
                        "{:<12} {:.1}°F ({:.1}°C)   H:{:.1}°F  L:{:.1}°F   {} {}",
                        w.city, w.current_f, w.current_c, w.high_f, w.low_f, w.emoji, w.description,
                    ))
                    .font(Font::MONOSPACE),
                );
            }
        }
        panel(col.into())
    }

    fn github(&self) -> Element<'_, Message> {
        // Mirror the view layout: body padding 10, row spacing 10, right column
        // is 2/3 of the row, panel padding 10.
        let content_w = (self.window_size.width - 20.0).max(0.0);
        let right_w = ((content_w - 10.0) * 2.0 / 3.0).max(0.0);
        let avail = (right_w - 20.0).max(0.0);

        panel(
            column![
                text(format!("GitHub ({})", self.snapshot.github.status))
                    .size(18.0)
                    .color(CYAN),
                github_emojis(&self.snapshot.github.days, avail),
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

/// Build the GitHub activity row: newest-first, tinted per level, truncated so
/// only legacy (oldest) days are dropped when width is tight.
fn github_emojis(days: &[(chrono::NaiveDate, u32)], avail: f32) -> Element<'_, Message> {
    // Approximate glyph+gap width; slightly conservative so we never wrap.
    const EMOJI_W: f32 = 22.0;
    const GAP: f32 = 4.0;

    let mut kept: Vec<(u32, &str)> = Vec::new();
    let mut used = 0.0;
    for (_, count) in days {
        let add = if kept.is_empty() { EMOJI_W } else { EMOJI_W + GAP };
        if !kept.is_empty() && used + add > avail {
            break;
        }
        used += add;
        kept.push((*count, GitHubActivity::emoji_for_count(*count)));
    }

    if kept.is_empty() {
        return text("...").into();
    }

    let mut r = row![].spacing(GAP);
    for (count, emoji) in kept {
        r = r.push(emoji_text(emoji).color(github_color(count)));
    }
    r.into()
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

/// Tint for a GitHub day's activity level, matching `emoji_for_count`'s tiers:
/// none (❌) red, light (✅) green, busy (🌟) yellow, heavy (🚀) cyan.
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

/// Height of the CPU-history wave, in points.
const WAVE_HEIGHT: f32 = 48.0;
/// Smoothing factor for the exponential moving average (0 = very smooth, 1 = raw).
const WAVE_EMA_ALPHA: f32 = 0.4;
/// Catmull-Rom subdivisions per sample gap (higher = smoother curve).
const WAVE_SUBDIVISIONS: usize = 8;

/// Canvas program that draws the CPU-load history as a smoothed, filled wave.
struct CpuWave {
    history: Vec<f32>,
}

impl<Message> canvas::Program<Message> for CpuWave {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let w = bounds.width;
        let h = bounds.height;

        // Faint baseline along the bottom.
        let baseline = Path::line(Point::new(0.0, h), Point::new(w, h));
        frame.stroke(
            &baseline,
            Stroke::default()
                .with_width(1.0)
                .with_color(Color {
                    r: 0.27,
                    g: 0.27,
                    b: 0.27,
                    a: 1.0,
                }),
        );

        if self.history.len() < 2 {
            frame.fill_text(canvas::Text {
                content: "collecting…".into(),
                position: Point::new(w / 2.0, h / 2.0),
                color: GRAY,
                size: 12.0.into(),
                horizontal_alignment: iced::alignment::Horizontal::Center,
                vertical_alignment: iced::alignment::Vertical::Center,
                ..canvas::Text::default()
            });
            return vec![frame.into_geometry()];
        }

        let smoothed = ema(&self.history, WAVE_EMA_ALPHA);
        let curve = catmull_rom(&smoothed, WAVE_SUBDIVISIONS);
        let points: Vec<Point> = curve
            .iter()
            .map(|&(x_frac, val)| {
                Point::new(
                    w * x_frac,
                    h - h * (val / 100.0).clamp(0.0, 1.0),
                )
            })
            .collect();

        let color = level_color(*self.history.last().unwrap(), 33.3, 66.6);
        let fill_color = Color {
            a: 48.0 / 255.0,
            ..color
        };

        // Closed path: curve left→right, then down the baseline back to the start.
        // x is monotonic, so a simple fill works (no need for a triangle strip).
        let area = Path::new(|b| {
            b.move_to(points[0]);
            for p in &points[1..] {
                b.line_to(*p);
            }
            let last = *points.last().unwrap();
            b.line_to(Point::new(last.x, h));
            b.line_to(Point::new(points[0].x, h));
            b.close();
        });
        frame.fill(&area, fill_color);

        let line = Path::new(|b| {
            b.move_to(points[0]);
            for p in &points[1..] {
                b.line_to(*p);
            }
        });
        frame.stroke(
            &line,
            Stroke::default().with_width(1.5).with_color(color),
        );

        vec![frame.into_geometry()]
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
