use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize, Clone)]
pub struct Config {
    pub locations: Vec<LocationConfig>,
    pub urls: UrlsConfig,
    pub refresh: RefreshConfig,
    pub display: DisplayConfig,
    #[serde(default)]
    pub currency: CurrencyConfig,
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
    #[serde(default = "default_currency_refresh_secs")]
    pub currency_secs: u64,
}

fn default_currency_refresh_secs() -> u64 {
    3600
}

#[derive(Deserialize, Clone)]
pub struct DisplayConfig {
    pub cpu_history_len: usize,
}

#[derive(Deserialize, Clone)]
pub struct CurrencyConfig {
    pub units: Vec<String>,
}

impl Default for CurrencyConfig {
    fn default() -> Self {
        Self {
            units: vec!["USD".to_string(), "EUR".to_string()],
        }
    }
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
                currency_secs: 3600,
            },
            display: DisplayConfig {
                cpu_history_len: 24,
            },
            currency: CurrencyConfig::default(),
        }
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|p| p.join(".config").join("tc-tui").join("config.toml"))
}

pub enum ConfigSource {
    File(PathBuf),
    Default(String),
}

impl ConfigSource {
    pub fn label(&self) -> String {
        match self {
            ConfigSource::File(p) => format!("cfg: {}", p.display()),
            ConfigSource::Default(reason) => format!("cfg: default ({})", reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Config::default ---

    #[test]
    fn default_has_two_locations() {
        let cfg = Config::default();
        assert_eq!(cfg.locations.len(), 2);
    }

    #[test]
    fn default_location_labels() {
        let cfg = Config::default();
        assert_eq!(cfg.locations[0].label, "St. Louis");
        assert_eq!(cfg.locations[1].label, "Granada");
    }

    #[test]
    fn default_st_louis_coords() {
        let cfg = Config::default();
        let loc = &cfg.locations[0];
        assert!((loc.lat - 38.6270).abs() < 0.001);
        assert!((loc.lon - (-90.1994)).abs() < 0.001);
    }

    #[test]
    fn default_granada_coords() {
        let cfg = Config::default();
        let loc = &cfg.locations[1];
        assert!((loc.lat - 37.1773).abs() < 0.001);
        assert!((loc.lon - (-3.5986)).abs() < 0.001);
    }

    #[test]
    fn default_has_four_urls() {
        let cfg = Config::default();
        assert_eq!(cfg.urls.sites.len(), 4);
    }

    #[test]
    fn default_refresh_intervals() {
        let cfg = Config::default();
        assert_eq!(cfg.refresh.weather_secs, 1800);
        assert_eq!(cfg.refresh.url_check_secs, 180);
        assert_eq!(cfg.refresh.cpu_sample_secs, 5);
        assert_eq!(cfg.refresh.currency_secs, 3600);
    }

    #[test]
    fn default_cpu_history_len() {
        let cfg = Config::default();
        assert_eq!(cfg.display.cpu_history_len, 24);
    }

    #[test]
    fn default_currency_units() {
        let cfg = Config::default();
        assert_eq!(cfg.currency.units, vec!["USD".to_string(), "EUR".to_string()]);
    }

    // --- TOML parsing ---

    #[test]
    fn parse_minimal_valid_toml() {
        let toml = r#"
            [[locations]]
            label = "Test City"
            lat   = 1.0
            lon   = 2.0

            [urls]
            sites = ["https://example.com"]

            [refresh]
            weather_secs    = 60
            url_check_secs  = 30
            cpu_sample_secs = 1
            currency_secs   = 3600

            [display]
            cpu_history_len = 10

            [currency]
            units = ["USD", "EUR"]
        "#;
        let cfg: Config = toml::from_str(toml).expect("should parse");
        assert_eq!(cfg.locations.len(), 1);
        assert_eq!(cfg.locations[0].label, "Test City");
        assert_eq!(cfg.urls.sites[0], "https://example.com");
        assert_eq!(cfg.refresh.weather_secs, 60);
        assert_eq!(cfg.display.cpu_history_len, 10);
        assert_eq!(cfg.refresh.currency_secs, 3600);
        assert_eq!(cfg.currency.units, vec!["USD".to_string(), "EUR".to_string()]);
    }

    #[test]
    fn parse_multiple_locations() {
        let toml = r#"
            [[locations]]
            label = "City A"
            lat   = 10.0
            lon   = 20.0

            [[locations]]
            label = "City B"
            lat   = 30.0
            lon   = 40.0

            [urls]
            sites = []

            [refresh]
            weather_secs    = 100
            url_check_secs  = 50
            cpu_sample_secs = 2
            currency_secs   = 3600

            [display]
            cpu_history_len = 5

            [currency]
            units = ["USD", "EUR"]
        "#;
        let cfg: Config = toml::from_str(toml).expect("should parse");
        assert_eq!(cfg.locations.len(), 2);
        assert_eq!(cfg.locations[1].label, "City B");
    }

    #[test]
    fn parse_without_currency_uses_default() {
        let toml = r#"
            [[locations]]
            label = "Test City"
            lat   = 1.0
            lon   = 2.0

            [urls]
            sites = ["https://example.com"]

            [refresh]
            weather_secs    = 60
            url_check_secs  = 30
            cpu_sample_secs = 1

            [display]
            cpu_history_len = 10
        "#;
        let cfg: Config = toml::from_str(toml).expect("should parse with currency default");
        assert_eq!(cfg.currency.units, vec!["USD".to_string(), "EUR".to_string()]);
        assert_eq!(cfg.refresh.currency_secs, 3600);
    }

    #[test]
    fn parse_invalid_toml_fails() {
        let bad = "this is not valid toml :::";
        let result: Result<Config, _> = toml::from_str(bad);
        assert!(result.is_err());
    }

    // --- ConfigSource::label ---

    #[test]
    fn config_source_file_label_contains_path() {
        let source = ConfigSource::File(PathBuf::from("/home/user/.config/tc-tui/config.toml"));
        assert!(source.label().contains("cfg:"));
        assert!(source.label().contains("config.toml"));
    }

    #[test]
    fn config_source_default_label_contains_reason() {
        let source = ConfigSource::Default("file not found".to_string());
        let label = source.label();
        assert!(label.contains("default"));
        assert!(label.contains("file not found"));
    }
}

pub fn load() -> (Config, ConfigSource) {
    let path = match config_path() {
        Some(p) => p,
        None => return (Config::default(), ConfigSource::Default("no config dir".to_string())),
    };

    let contents = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => return (
            Config::default(),
            ConfigSource::Default(format!("{}: {}", path.display(), e.kind())),
        ),
    };

    match toml::from_str(&contents) {
        Ok(cfg) => (cfg, ConfigSource::File(path)),
        Err(e) => {
            let reason = format!("parse error in {}: {}", path.display(), e);
            (Config::default(), ConfigSource::Default(reason))
        }
    }
}
