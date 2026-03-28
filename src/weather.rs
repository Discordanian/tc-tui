use serde::Deserialize;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::{GRANADA_LAT, GRANADA_LON, ST_LOUIS_LAT, ST_LOUIS_LON};

#[derive(Clone)]
pub struct WeatherInfo {
    pub city: String,
    pub current_f: f64,
    pub current_c: f64,
    pub high_f: f64,
    pub high_c: f64,
    pub low_f: f64,
    pub low_c: f64,
    pub description: String,
    pub emoji: String,
}

impl WeatherInfo {
    pub fn pending(city: &str) -> Self {
        WeatherInfo {
            city: city.to_string(),
            current_f: 0.0,
            current_c: 0.0,
            high_f: 0.0,
            high_c: 0.0,
            low_f: 0.0,
            low_c: 0.0,
            description: "...".to_string(),
            emoji: "⏳".to_string(),
        }
    }
}

#[derive(Deserialize)]
struct OpenMeteoResponse {
    current: OpenMeteoCurrent,
    daily: OpenMeteoDaily,
}

#[derive(Deserialize)]
struct OpenMeteoCurrent {
    temperature_2m: f64,
    weather_code: u32,
}

#[derive(Deserialize)]
struct OpenMeteoDaily {
    temperature_2m_max: Vec<f64>,
    temperature_2m_min: Vec<f64>,
}

fn c_to_f(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

fn weather_code_to_emoji_desc(code: u32) -> (&'static str, &'static str) {
    match code {
        0 => ("☀️", "Clear"),
        1 => ("🌤️", "Mainly clear"),
        2 => ("⛅", "Partly cloudy"),
        3 => ("☁️", "Overcast"),
        45 | 48 => ("🌫️", "Fog"),
        51 | 53 | 55 => ("🌦️", "Drizzle"),
        56 | 57 => ("🌧️", "Freezing drizzle"),
        61 | 63 | 65 => ("🌧️", "Rain"),
        66 | 67 => ("🌨️", "Freezing rain"),
        71 | 73 | 75 => ("❄️", "Snow"),
        77 => ("🌨️", "Snow grains"),
        80 | 81 | 82 => ("🌦️", "Showers"),
        85 | 86 => ("🌨️", "Snow showers"),
        95 => ("⛈️", "Thunderstorm"),
        96 | 99 => ("⛈️", "Thunderstorm/hail"),
        _ => ("🌡️", "Unknown"),
    }
}

fn fetch_weather(city: &str, lat: f64, lon: f64) -> Option<WeatherInfo> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast\
         ?latitude={lat}&longitude={lon}\
         &current=temperature_2m,weather_code\
         &daily=temperature_2m_max,temperature_2m_min\
         &temperature_unit=celsius\
         &timezone=auto\
         &forecast_days=1"
    );

    let resp: OpenMeteoResponse = ureq::get(&url).call().ok()?.into_json().ok()?;

    let current_c = resp.current.temperature_2m;
    let high_c = *resp.daily.temperature_2m_max.first()?;
    let low_c = *resp.daily.temperature_2m_min.first()?;
    let (emoji, desc) = weather_code_to_emoji_desc(resp.current.weather_code);

    Some(WeatherInfo {
        city: city.to_string(),
        current_f: c_to_f(current_c),
        current_c,
        high_f: c_to_f(high_c),
        high_c,
        low_f: c_to_f(low_c),
        low_c,
        description: desc.to_string(),
        emoji: emoji.to_string(),
    })
}

pub fn spawn_weather_fetcher(
    weather: Arc<Mutex<Vec<WeatherInfo>>>,
    refresh_rx: mpsc::Receiver<()>,
) {
    thread::spawn(move || {
        loop {
            let locations = [
                ("St. Louis", ST_LOUIS_LAT, ST_LOUIS_LON),
                ("Granada", GRANADA_LAT, GRANADA_LON),
            ];
            let results: Vec<WeatherInfo> = locations
                .iter()
                .map(|(city, lat, lon)| {
                    fetch_weather(city, *lat, *lon).unwrap_or_else(|| {
                        let mut w = WeatherInfo::pending(city);
                        w.description = "Error".to_string();
                        w.emoji = "❌".to_string();
                        w
                    })
                })
                .collect();

            if let Ok(mut w) = weather.lock() {
                *w = results;
            }

            match refresh_rx.recv_timeout(Duration::from_secs(1800)) {
                Ok(()) => continue,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}
