use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize, Clone)]
pub struct Config {
    pub locations: Vec<LocationConfig>,
    pub urls: UrlsConfig,
    pub refresh: RefreshConfig,
    pub display: DisplayConfig,
}

#[derive(Deserialize, Clone)]
pub struct LocationConfig {
    pub label: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Deserialize, Clone)]
pub struct UrlsConfig {
    pub sites: Vec<String>,
}

#[derive(Deserialize, Clone)]
pub struct RefreshConfig {
    pub weather_secs: u64,
    pub url_check_secs: u64,
    pub cpu_sample_secs: u64,
}

#[derive(Deserialize, Clone)]
pub struct DisplayConfig {
    pub cpu_history_len: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            locations: vec![
                LocationConfig {
                    label: "St. Louis".to_string(),
                    lat: 38.6270,
                    lon: -90.1994,
                },
                LocationConfig {
                    label: "Granada".to_string(),
                    lat: 37.1773,
                    lon: -3.5986,
                },
            ],
            urls: UrlsConfig {
                sites: vec![
                    "https://tangentialcold.com".to_string(),
                    "https://babilonia.tangentialcold.com".to_string(),
                    "https://annaschwind.com".to_string(),
                    "https://slithytoves.org".to_string(),
                ],
            },
            refresh: RefreshConfig {
                weather_secs: 1800,
                url_check_secs: 180,
                cpu_sample_secs: 5,
            },
            display: DisplayConfig {
                cpu_history_len: 24,
            },
        }
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("tc-tui").join("config.toml"))
}

pub fn load() -> Config {
    let path = match config_path() {
        Some(p) => p,
        None => return Config::default(),
    };

    let contents = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Config::default(),
    };

    match toml::from_str(&contents) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Warning: failed to parse config at {}: {}", path.display(), e);
            Config::default()
        }
    }
}
