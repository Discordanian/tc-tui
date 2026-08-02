use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn fetch_ip_city() -> String {
    #[derive(serde::Deserialize)]
    struct IpInfo {
        city: Option<String>,
        region: Option<String>,
    }
    match ureq::get("https://ipinfo.io/json").call() {
        Ok(mut resp) => match resp.body_mut().read_json::<IpInfo>() {
            Ok(info) => match (info.city, info.region) {
                (Some(c), Some(r)) => format!("{}, {}", c, r),
                (Some(c), None) => c,
                _ => "Unknown".to_string(),
            },
            Err(_) => "Unknown".to_string(),
        },
        Err(_) => "Unknown".to_string(),
    }
}

pub fn spawn_ip_city_fetcher(
    city: Arc<Mutex<String>>,
    refresh_rx: mpsc::Receiver<()>,
    interval_secs: u64,
) {
    thread::spawn(move || {
        loop {
            let result = fetch_ip_city();
            if let Ok(mut c) = city.lock() {
                *c = result;
            }
            match refresh_rx.recv_timeout(Duration::from_secs(interval_secs)) {
                Ok(()) => continue,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}

pub fn vpn_active() -> bool {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .iter()
        .any(|iface| {
            let n = iface.name.as_str();
            n.starts_with("tun") || n.starts_with("tap") || n.starts_with("utun")
                || n.starts_with("wg") || n.starts_with("ppp")
        })
}

/// Poll the VPN interface state on an interval, updating shared state. Sending
/// on the refresh channel forces an immediate re-check.
pub fn spawn_vpn_monitor(
    vpn: Arc<Mutex<bool>>,
    refresh_rx: mpsc::Receiver<()>,
    interval_secs: u64,
) {
    thread::spawn(move || loop {
        let active = vpn_active();
        if let Ok(mut v) = vpn.lock() {
            *v = active;
        }
        match refresh_rx.recv_timeout(Duration::from_secs(interval_secs)) {
            Ok(()) => continue,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    });
}

pub fn fetch_status(url: &str) -> String {
    match ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .new_agent()
        .get(url)
        .call()
    {
        Ok(resp) => resp.status().as_u16().to_string(),
        Err(ureq::Error::StatusCode(code)) => code.to_string(),
        Err(_) => "ERR".to_string(),
    }
}

pub fn reset_statuses(statuses: &Arc<Mutex<Vec<(String, String)>>>) {
    if let Ok(mut s) = statuses.lock() {
        for (code, _) in s.iter_mut() {
            *code = "...".to_string();
        }
    }
}

pub fn refresh_statuses(statuses: &Arc<Mutex<Vec<(String, String)>>>) {
    let urls: Vec<String> = statuses
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|(_, url)| url.clone())
        .collect();

    let results: Vec<(String, String)> = urls
        .iter()
        .map(|url| {
            let status = fetch_status(url);
            (status, url.clone())
        })
        .collect();

    if let Ok(mut s) = statuses.lock() {
        *s = results;
    }
}

pub fn spawn_status_checker(
    statuses: Arc<Mutex<Vec<(String, String)>>>,
    refresh_rx: mpsc::Receiver<()>,
    interval_secs: u64,
) {
    thread::spawn(move || {
        loop {
            refresh_statuses(&statuses);

            match refresh_rx.recv_timeout(Duration::from_secs(interval_secs)) {
                Ok(()) => continue,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_statuses(entries: &[(&str, &str)]) -> Arc<Mutex<Vec<(String, String)>>> {
        Arc::new(Mutex::new(
            entries.iter().map(|(c, u)| (c.to_string(), u.to_string())).collect(),
        ))
    }

    #[test]
    fn reset_statuses_sets_all_codes_to_pending() {
        let statuses = make_statuses(&[
            ("200", "https://example.com"),
            ("404", "https://missing.example.com"),
            ("ERR", "https://broken.example.com"),
        ]);

        reset_statuses(&statuses);

        let locked = statuses.lock().unwrap();
        for (code, _) in locked.iter() {
            assert_eq!(code, "...", "Expected '...' but got '{code}'");
        }
    }

    #[test]
    fn reset_statuses_preserves_urls() {
        let urls = vec!["https://example.com", "https://other.example.com"];
        let statuses = make_statuses(&[("200", urls[0]), ("500", urls[1])]);

        reset_statuses(&statuses);

        let locked = statuses.lock().unwrap();
        let stored_urls: Vec<&str> = locked.iter().map(|(_, u)| u.as_str()).collect();
        assert_eq!(stored_urls, urls);
    }

    #[test]
    fn reset_statuses_on_empty_list_is_noop() {
        let statuses = make_statuses(&[]);
        reset_statuses(&statuses);
        let locked = statuses.lock().unwrap();
        assert!(locked.is_empty());
    }

    #[test]
    fn reset_statuses_idempotent() {
        let statuses = make_statuses(&[("200", "https://example.com")]);
        reset_statuses(&statuses);
        reset_statuses(&statuses);
        let locked = statuses.lock().unwrap();
        assert_eq!(locked[0].0, "...");
    }
}
