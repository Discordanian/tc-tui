use std::sync::{mpsc, Arc, Mutex};

use crate::config::{currency_units, Config, ConfigSource};
use crate::currency::{spawn_currency_fetcher, CurrencyRate};
use crate::github::{spawn_github_fetcher, GitHubActivity};
use crate::net::{
    reset_statuses, spawn_ip_city_fetcher, spawn_status_checker, spawn_vpn_monitor,
};
use crate::system::{spawn_system_monitor, SysSnapshot, SysState};
use crate::weather::{spawn_weather_fetcher, WeatherInfo};

const IP_CITY_REFRESH_SECS: u64 = 300;
const VPN_REFRESH_SECS: u64 = 300;

/// Owns all shared, background-updated application state and the handles used to
/// force a refresh. Construct with [`App::new`], start the background workers
/// with [`App::spawn_fetchers`], then poll [`App::snapshot`] from any frontend.
pub struct App {
    statuses: Arc<Mutex<Vec<(String, String)>>>,
    weather: Arc<Mutex<Vec<WeatherInfo>>>,
    ip_city: Arc<Mutex<String>>,
    currency_rates: Arc<Mutex<(CurrencyRate, CurrencyRate)>>,
    github_activity: Arc<Mutex<GitHubActivity>>,
    vpn: Arc<Mutex<bool>>,
    sys: Arc<Mutex<SysState>>,
    refresh_senders: Vec<mpsc::Sender<()>>,
    pub cfg: Config,
    pub cfg_source: ConfigSource,
}

/// A plain, `Clone`-able view of all application data at a moment in time. Has
/// no dependency on any UI toolkit, so every frontend renders from the same
/// structure.
#[derive(Clone)]
pub struct Snapshot {
    pub statuses: Vec<(String, String)>,
    pub weather: Vec<WeatherInfo>,
    pub ip_city: String,
    pub vpn: bool,
    pub currency_rates: (CurrencyRate, CurrencyRate),
    pub github: GitHubActivity,
    pub sys: SysSnapshot,
    pub cpu_history: Vec<f32>,
}

impl App {
    /// Build the shared state seeded with placeholder values derived from the
    /// config. No threads are started until [`App::spawn_fetchers`] is called.
    pub fn new(cfg: Config, cfg_source: ConfigSource) -> Self {
        let (currency_a, currency_b) = currency_units(&cfg);

        let statuses = Arc::new(Mutex::new(
            cfg.urls
                .sites
                .iter()
                .map(|url| ("...".to_string(), url.clone()))
                .collect(),
        ));

        let weather = Arc::new(Mutex::new(
            cfg.locations
                .iter()
                .map(|l| WeatherInfo::pending(&l.label))
                .collect(),
        ));

        let ip_city = Arc::new(Mutex::new("...".to_string()));
        let currency_rates = Arc::new(Mutex::new((
            CurrencyRate::pending(&currency_a, &currency_b),
            CurrencyRate::pending(&currency_b, &currency_a),
        )));
        let github_activity = Arc::new(Mutex::new(GitHubActivity::pending()));
        let vpn = Arc::new(Mutex::new(false));
        let sys = Arc::new(Mutex::new(SysState::empty()));

        App {
            statuses,
            weather,
            ip_city,
            currency_rates,
            github_activity,
            vpn,
            sys,
            refresh_senders: Vec::new(),
            cfg,
            cfg_source,
        }
    }

    /// Spawn every background worker (URL checks, weather, IP/city, currency,
    /// GitHub, VPN, system monitor) and record their refresh channels.
    pub fn spawn_fetchers(&mut self) {
        let (currency_a, currency_b) = currency_units(&self.cfg);

        let (status_tx, status_rx) = mpsc::channel();
        let (weather_tx, weather_rx) = mpsc::channel();
        let (ip_city_tx, ip_city_rx) = mpsc::channel();
        let (currency_tx, currency_rx) = mpsc::channel();
        let (github_tx, github_rx) = mpsc::channel();
        let (vpn_tx, vpn_rx) = mpsc::channel();
        let (sys_tx, sys_rx) = mpsc::channel();

        spawn_status_checker(
            Arc::clone(&self.statuses),
            status_rx,
            self.cfg.refresh.url_check_secs,
        );
        spawn_weather_fetcher(
            Arc::clone(&self.weather),
            weather_rx,
            self.cfg.locations.clone(),
            self.cfg.refresh.weather_secs,
        );
        spawn_ip_city_fetcher(Arc::clone(&self.ip_city), ip_city_rx, IP_CITY_REFRESH_SECS);
        spawn_currency_fetcher(
            Arc::clone(&self.currency_rates),
            currency_rx,
            currency_a,
            currency_b,
            self.cfg.refresh.currency_secs,
        );
        spawn_github_fetcher(
            Arc::clone(&self.github_activity),
            github_rx,
            self.cfg.github.username.clone(),
            self.cfg.github.token.clone(),
            self.cfg.refresh.github_secs,
        );
        spawn_vpn_monitor(Arc::clone(&self.vpn), vpn_rx, VPN_REFRESH_SECS);
        spawn_system_monitor(
            Arc::clone(&self.sys),
            sys_rx,
            self.cfg.display.cpu_history_len,
            self.cfg.refresh.cpu_sample_secs,
        );

        self.refresh_senders = vec![
            status_tx, weather_tx, ip_city_tx, currency_tx, github_tx, vpn_tx, sys_tx,
        ];
    }

    /// Force every worker to refresh immediately. URL statuses are reset to the
    /// pending placeholder so the change is visible right away.
    pub fn refresh_all(&self) {
        reset_statuses(&self.statuses);
        for tx in &self.refresh_senders {
            let _ = tx.send(());
        }
    }

    /// Take a consistent, `Clone`-able snapshot of all current data.
    pub fn snapshot(&self) -> Snapshot {
        let statuses = self
            .statuses
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let weather = self
            .weather
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let ip_city = self
            .ip_city
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let currency_rates = self
            .currency_rates
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let github = self
            .github_activity
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let vpn = *self.vpn.lock().unwrap_or_else(|e| e.into_inner());
        let sys_state = self.sys.lock().unwrap_or_else(|e| e.into_inner()).clone();

        Snapshot {
            statuses,
            weather,
            ip_city,
            vpn,
            currency_rates,
            github,
            sys: sys_state.snapshot,
            cpu_history: sys_state.cpu_history,
        }
    }
}
