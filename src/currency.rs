use serde::Deserialize;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub struct CurrencyRate {
    pub base: String,
    pub quote: String,
    pub rate: f64,
    pub status: String,
}

impl CurrencyRate {
    pub fn pending(base: &str, quote: &str) -> Self {
        Self {
            base: base.to_string(),
            quote: quote.to_string(),
            rate: 0.0,
            status: "...".to_string(),
        }
    }
}

#[derive(Deserialize)]
struct FrankfurterResponse {
    rates: std::collections::HashMap<String, f64>,
}

fn fetch_rate(base: &str, quote: &str) -> Option<f64> {
    let url = format!(
        "https://api.frankfurter.app/latest?amount=1&from={}&to={}",
        base, quote
    );
    let resp: FrankfurterResponse = ureq::get(&url).call().ok()?.into_json().ok()?;
    resp.rates.get(quote).copied()
}

fn fetch_pair(base: &str, quote: &str) -> CurrencyRate {
    match fetch_rate(base, quote) {
        Some(rate) => CurrencyRate {
            base: base.to_string(),
            quote: quote.to_string(),
            rate,
            status: "OK".to_string(),
        },
        None => {
            let mut pending = CurrencyRate::pending(base, quote);
            pending.status = "ERR".to_string();
            pending
        }
    }
}

pub fn spawn_currency_fetcher(
    rates: Arc<Mutex<(CurrencyRate, CurrencyRate)>>,
    refresh_rx: mpsc::Receiver<()>,
    currency_a: String,
    currency_b: String,
    interval_secs: u64,
) {
    thread::spawn(move || loop {
        let ab = fetch_pair(&currency_a, &currency_b);
        let ba = fetch_pair(&currency_b, &currency_a);

        if let Ok(mut r) = rates.lock() {
            *r = (ab, ba);
        }

        match refresh_rx.recv_timeout(Duration::from_secs(interval_secs)) {
            Ok(()) => continue,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_uses_placeholder_status() {
        let rate = CurrencyRate::pending("USD", "EUR");
        assert_eq!(rate.base, "USD");
        assert_eq!(rate.quote, "EUR");
        assert_eq!(rate.rate, 0.0);
        assert_eq!(rate.status, "...");
    }
}
