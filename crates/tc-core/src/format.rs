use chrono::{DateTime, Datelike, Utc, Weekday};
use chrono_tz::{America::Chicago, Europe::Madrid};

pub fn weekday_name_spanish(w: Weekday) -> &'static str {
    match w {
        Weekday::Mon => "Lunes",
        Weekday::Tue => "Martes",
        Weekday::Wed => "Miércoles",
        Weekday::Thu => "Jueves",
        Weekday::Fri => "Viernes",
        Weekday::Sat => "Sábado",
        Weekday::Sun => "Domingo",
    }
}

/// Weekday + date: Spanish uses Europe/Madrid; English uses America/Chicago
/// (St. Louis). Language toggles each UTC minute.
pub fn format_header_day_date(now: DateTime<Utc>) -> String {
    let use_spanish = (now.timestamp() / 60) % 2 != 0;
    if use_spanish {
        let local = now.with_timezone(&Madrid);
        let weekday = weekday_name_spanish(local.weekday());
        let date = local.format("%Y-%m-%d");
        format!("{weekday} {date}")
    } else {
        let local = now.with_timezone(&Chicago);
        let weekday = local.format("%A");
        let date = local.format("%Y-%m-%d");
        format!("{weekday} {date}")
    }
}

pub fn parse_currency_input(raw: &str) -> Option<f64> {
    if raw.is_empty() {
        None
    } else {
        raw.parse::<f64>().ok()
    }
}

pub fn render_currency_value(value: Option<f64>, unit: &str) -> String {
    match value {
        Some(v) => format!("{:.4} {}", v, unit),
        None => format!("... {}", unit),
    }
}

pub fn currency_status_icon(currency: &str, status: &str) -> &'static str {
    if status == "OK" {
        currency_emoji(currency)
    } else {
        "⚠️"
    }
}

pub fn currency_emoji(currency: &str) -> &'static str {
    match currency {
        "USD" => "🇺🇸",
        "EUR" => "🇪🇺",
        "GBP" => "🇬🇧",
        "JPY" => "🇯🇵",
        "CAD" => "🇨🇦",
        "AUD" => "🇦🇺",
        "CHF" => "🇨🇭",
        _ => "💱",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn spanish_weekday_names() {
        assert_eq!(weekday_name_spanish(Weekday::Mon), "Lunes");
        assert_eq!(weekday_name_spanish(Weekday::Tue), "Martes");
        assert_eq!(weekday_name_spanish(Weekday::Wed), "Miércoles");
        assert_eq!(weekday_name_spanish(Weekday::Thu), "Jueves");
        assert_eq!(weekday_name_spanish(Weekday::Fri), "Viernes");
        assert_eq!(weekday_name_spanish(Weekday::Sat), "Sábado");
        assert_eq!(weekday_name_spanish(Weekday::Sun), "Domingo");
    }

    // --- parse_currency_input ---

    #[test]
    fn parse_currency_input_empty_is_none() {
        assert_eq!(parse_currency_input(""), None);
    }

    #[test]
    fn parse_currency_input_valid_integer() {
        assert_eq!(parse_currency_input("42"), Some(42.0));
    }

    #[test]
    fn parse_currency_input_valid_float() {
        let v = parse_currency_input("3.14").unwrap();
        assert!((v - 3.14).abs() < 1e-9);
    }

    #[test]
    fn parse_currency_input_invalid_is_none() {
        assert_eq!(parse_currency_input("abc"), None);
        assert_eq!(parse_currency_input("1.2.3"), None);
    }

    // --- render_currency_value ---

    #[test]
    fn render_currency_value_some_formats_four_decimals() {
        assert_eq!(render_currency_value(Some(1.5), "EUR"), "1.5000 EUR");
    }

    #[test]
    fn render_currency_value_none_uses_placeholder() {
        assert_eq!(render_currency_value(None, "GBP"), "... GBP");
    }

    // --- format_header_day_date ---

    #[test]
    fn header_day_date_includes_date_and_weekday_word() {
        let t = NaiveDate::from_ymd_opt(2024, 6, 15)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc();
        let s = format_header_day_date(t);
        assert!(s.contains("2024-06-15"));
        assert!(s.contains("Saturday") || s.contains("Sábado"));
    }

    #[test]
    fn header_day_date_toggles_weekday_language_each_utc_minute() {
        let t0 = NaiveDate::from_ymd_opt(2024, 6, 15)
            .unwrap()
            .and_hms_opt(14, 30, 0)
            .unwrap()
            .and_utc();
        let t1 = t0 + chrono::Duration::minutes(1);
        let w0 = format_header_day_date(t0)
            .split_whitespace()
            .next()
            .unwrap()
            .to_string();
        let w1 = format_header_day_date(t1)
            .split_whitespace()
            .next()
            .unwrap()
            .to_string();
        assert_ne!(
            w0, w1,
            "adjacent UTC minutes should alternate English/Spanish weekday"
        );
    }
}
