//! Shared, UI-agnostic binary assets bundled into the app.
//!
//! These are exposed as raw bytes so each frontend can register them in
//! whatever way its toolkit expects (egui `FontData`, iced `Font`, Slint font
//! embedding, etc.). Keeping the bytes here means every frontend uses the exact
//! same asset without duplicating it per crate.

/// Monochrome Noto Emoji (variable weight) as a `.ttf`.
///
/// egui/epaint (and most other Rust GUI toolkits) can only rasterize
/// monochrome glyph outlines, so the default/system *color* emoji fonts render
/// nothing. This font provides broad emoji coverage — including the weather
/// glyphs (☀️ ⛅ 🌧️ ⛈️ ❄️ …) — as single-color outlines that do render.
///
/// Licensed under the SIL Open Font License 1.1.
pub const NOTO_EMOJI_TTF: &[u8] = include_bytes!("../assets/NotoEmoji.ttf");
