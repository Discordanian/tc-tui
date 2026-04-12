use chrono::{NaiveDate, Utc};
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct GitHubActivity {
    pub days: Vec<(NaiveDate, u32)>,
    pub status: String,
}

impl GitHubActivity {
    pub fn pending() -> Self {
        Self {
            days: Vec::new(),
            status: "...".to_string(),
        }
    }

    pub fn emoji_for_count(count: u32) -> &'static str {
        match count {
            0 => "❌",
            1..=3 => "✅",
            4..=6 => "🌟",
            _ => "🚀",
        }
    }
}

fn fetch_contributions(username: &str) -> Result<Vec<(NaiveDate, u32)>, String> {
    let url = format!("https://github.com/users/{}/contributions", username);
    let resp = ureq::get(&url)
        .call()
        .map_err(|e| format!("HTTP error: {e}"))?;
    let body = resp.into_string().map_err(|e| format!("Read error: {e}"))?;
    parse_contribution_html(&body)
}

fn parse_contribution_html(html: &str) -> Result<Vec<(NaiveDate, u32)>, String> {
    let mut days: HashMap<NaiveDate, u32> = HashMap::new();
    let mut last_date: Option<NaiveDate> = None;

    for line in html.lines() {
        let trimmed = line.trim();

        if let Some(date_str) = extract_attr(trimmed, "data-date") {
            if let Ok(d) = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") {
                last_date = Some(d);
            }
        }

        if trimmed.contains("<tool-tip") {
            if let Some(date) = last_date.take() {
                let count = parse_tooltip_count(trimmed);
                days.insert(date, count);
            }
        }
    }

    if days.is_empty() {
        return Err("No contribution data found".to_string());
    }

    let today = Utc::now().date_naive();
    let mut result: Vec<(NaiveDate, u32)> = Vec::new();
    for i in 0..366 {
        let d = today - chrono::Duration::days(i);
        if let Some(&count) = days.get(&d) {
            result.push((d, count));
        }
    }

    Ok(result)
}

fn extract_attr(line: &str, attr: &str) -> Option<String> {
    let needle = format!("{}=\"", attr);
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Parse count from tooltip text like "28 contributions on April 13th."
/// or "No contributions on May 4th." or "1 contribution on ..."
fn parse_tooltip_count(line: &str) -> u32 {
    let text = strip_tags(line);
    let text = text.trim();
    if text.starts_with("No ") {
        return 0;
    }
    text.split_whitespace()
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for ch in s.chars() {
        match ch {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(ch),
            _ => {}
        }
    }
    out
}

pub fn spawn_github_fetcher(
    activity: Arc<Mutex<GitHubActivity>>,
    refresh_rx: mpsc::Receiver<()>,
    username: String,
    interval_secs: u64,
) {
    thread::spawn(move || loop {
        let result = fetch_contributions(&username);
        if let Ok(mut a) = activity.lock() {
            match result {
                Ok(days) => {
                    a.days = days;
                    a.status = username.clone();
                }
                Err(e) => {
                    a.status = e;
                }
            }
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
    fn emoji_zero_is_red_x() {
        assert_eq!(GitHubActivity::emoji_for_count(0), "❌");
    }

    #[test]
    fn emoji_1_to_3_is_checkmark() {
        for c in 1..=3 {
            assert_eq!(GitHubActivity::emoji_for_count(c), "✅");
        }
    }

    #[test]
    fn emoji_4_to_6_is_star() {
        for c in 4..=6 {
            assert_eq!(GitHubActivity::emoji_for_count(c), "🌟");
        }
    }

    #[test]
    fn emoji_above_6_is_rocket() {
        for c in [7, 10, 50, 100] {
            assert_eq!(GitHubActivity::emoji_for_count(c), "🚀");
        }
    }

    #[test]
    fn extract_attr_finds_value() {
        let line = r#"<td tabindex="0" data-ix="0" data-date="2025-04-13" id="contribution-day-component-0-0" data-level="3" class="ContributionCalendar-day"></td>"#;
        assert_eq!(extract_attr(line, "data-date"), Some("2025-04-13".to_string()));
        assert_eq!(extract_attr(line, "data-level"), Some("3".to_string()));
    }

    #[test]
    fn extract_attr_missing_returns_none() {
        let line = r#"<td class="day">"#;
        assert_eq!(extract_attr(line, "data-date"), None);
    }

    #[test]
    fn parse_tooltip_count_numeric() {
        let line = r#"<tool-tip class="sr-only">28 contributions on April 13th.</tool-tip>"#;
        assert_eq!(parse_tooltip_count(line), 28);
    }

    #[test]
    fn parse_tooltip_count_singular() {
        let line = r#"<tool-tip class="sr-only">1 contribution on April 20th.</tool-tip>"#;
        assert_eq!(parse_tooltip_count(line), 1);
    }

    #[test]
    fn parse_tooltip_count_none() {
        let line = r#"<tool-tip class="sr-only">No contributions on May 4th.</tool-tip>"#;
        assert_eq!(parse_tooltip_count(line), 0);
    }

    #[test]
    fn parse_contribution_html_real_format() {
        let html = r#"
<td tabindex="0" data-ix="0" data-date="2025-04-13" id="contribution-day-component-0-0" data-level="3" class="ContributionCalendar-day"></td>
  <tool-tip id="tooltip-1" for="contribution-day-component-0-0" class="sr-only">28 contributions on April 13th.</tool-tip>
<td tabindex="0" data-ix="1" data-date="2025-04-14" id="contribution-day-component-0-1" data-level="0" class="ContributionCalendar-day"></td>
  <tool-tip id="tooltip-2" for="contribution-day-component-0-1" class="sr-only">No contributions on April 14th.</tool-tip>
<td tabindex="0" data-ix="2" data-date="2025-04-15" id="contribution-day-component-0-2" data-level="1" class="ContributionCalendar-day"></td>
  <tool-tip id="tooltip-3" for="contribution-day-component-0-2" class="sr-only">3 contributions on April 15th.</tool-tip>
        "#;
        let result = parse_contribution_html(html);
        assert!(result.is_ok());
        let days = result.unwrap();
        let map: HashMap<NaiveDate, u32> = days.into_iter().collect();
        assert_eq!(map[&NaiveDate::from_ymd_opt(2025, 4, 13).unwrap()], 28);
        assert_eq!(map[&NaiveDate::from_ymd_opt(2025, 4, 14).unwrap()], 0);
        assert_eq!(map[&NaiveDate::from_ymd_opt(2025, 4, 15).unwrap()], 3);
    }

    #[test]
    fn parse_contribution_html_empty() {
        let html = "<html><body>nothing here</body></html>";
        let result = parse_contribution_html(html);
        assert!(result.is_err());
    }

    #[test]
    fn strip_tags_removes_html() {
        assert_eq!(strip_tags("<b>hello</b> world"), "hello world");
        assert_eq!(strip_tags("no tags"), "no tags");
    }

    #[test]
    fn pending_has_empty_days() {
        let a = GitHubActivity::pending();
        assert!(a.days.is_empty());
        assert_eq!(a.status, "...");
    }
}
