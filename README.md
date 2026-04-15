# tc-tui

Tangential Cold TUI — a terminal dashboard built with [Ratatui](https://ratatui.rs) that displays system info, weather, website health, currency conversion, and GitHub activity at a glance.

## Layout

The screen is divided into three horizontal bands: a header bar, a two-column body, and a footer menu bar.

```
┌──────────────────────────────────────────────────────────────┐
│  🇪🇸 HH:MM │ 🇺🇸 HH:MM    (city) 🔒 hostname    Day YYYY-MM-DD │  ← Header
├────────────────────┬─────────────────────────────────────────┤
│  URL Status      │  Weather                                  │
│  ────────────    │  ──────────────────────────────────────── │
│  System Info     │  GitHub (username)                        │
│  ────────────    │  ──────────────────────────────────────── │
│  CPU History     │                                           │
│  ────────────    │  Tangential Cold TUI                      │
│  Currency        │                                           │
│                  │                                           │
├──────────────────┴───────────────────────────────────────────┤
│  q Quit  r Refresh  cfg: ...                                 │  ← Footer
└──────────────────────────────────────────────────────────────┘
```

### Header

- Spain and US Central clocks
- Current IP-derived city with VPN lock/unlock indicator and hostname
- Day of the week (alternates English/Spanish each UTC minute) and date in Madrid time

### Left Column

- **URL Status** — HTTP status codes for configured sites (green 200, yellow 3xx, red 4xx/5xx)
- **System Info** — CPU count, CPU load, RAM used/total
- **CPU History** — color-coded bar graph (green/yellow/red) sampled at a configurable interval
- **Currency** — interactive two-way converter between two configured currencies; use Tab/Up/Down to switch rows, type digits to enter amounts

### Right Column

- **Weather** — current temperature (F/C), daily high/low, emoji and description for each configured location
- **GitHub** — emoji row showing daily contribution activity for a configured GitHub user, today first going backwards in time:
  - ❌ = 0 contributions
  - ✅ = 1–3 contributions
  - 🌟 = 4–6 contributions
  - 🚀 = 7+ contributions
- **Main area** — reserved for future content

### Footer

- Keybindings: `q` to quit, `r` to force-refresh all panels
- Shows the active config file path (green) or why defaults are being used (yellow)

## Keybindings

| Key | Action |
|---|---|
| `q` | Quit |
| `r` | Force-refresh all panels (URL checks, weather, IP/city, currency, GitHub) |
| `Tab` / `Up` / `Down` | Toggle active currency input row |
| `0`–`9`, `.` | Type into the active currency input |
| `Backspace` | Delete last character from the active currency input |

## Configuration

tc-tui reads its configuration from:

```
~/.config/tc-tui/config.toml
```

If the file is missing or cannot be parsed, built-in defaults are used. The footer bar shows which config is active.

A reference config file is included in the repository as `config.toml`.

### `[[locations]]`

One or more weather locations. Each entry requires all three fields.

```toml
[[locations]]
label = "St. Louis"
lat   = 38.6270
lon   = -90.1994

[[locations]]
label = "Granada"
lat   = 37.1773
lon   = -3.5986
```

| Field | Type | Description |
|---|---|---|
| `label` | string | Display name shown in the Weather panel |
| `lat` | float | Latitude (decimal degrees) |
| `lon` | float | Longitude (decimal degrees) |

### `[urls]`

Sites to monitor with HTTP GET requests. Each site's status code is displayed in the URL Status table.

```toml
[urls]
sites = [
    "https://tangentialcold.com",
    "https://babilonia.tangentialcold.com",
    "https://annaschwind.com",
    "https://slithytoves.org",
]
```

| Field | Type | Description |
|---|---|---|
| `sites` | array of strings | URLs to health-check |

### `[refresh]`

Controls how often each background fetcher re-polls its data source. All values are in seconds.

```toml
[refresh]
weather_secs    = 1800   # 30 minutes
url_check_secs  = 180    # 3 minutes
cpu_sample_secs = 5      # 5 seconds
currency_secs   = 3600   # 1 hour
github_secs     = 1800   # 30 minutes
```

| Field | Type | Default | Description |
|---|---|---|---|
| `weather_secs` | integer | 1800 | Weather data refresh interval |
| `url_check_secs` | integer | 180 | URL health-check interval |
| `cpu_sample_secs` | integer | 5 | CPU load sampling interval (also controls bar graph resolution) |
| `currency_secs` | integer | 3600 | Currency exchange rate refresh interval |
| `github_secs` | integer | 1800 | GitHub contribution data refresh interval |

### `[display]`

Visual tuning parameters.

```toml
[display]
cpu_history_len = 24
```

| Field | Type | Default | Description |
|---|---|---|---|
| `cpu_history_len` | integer | 24 | Number of bars shown in the CPU History graph. Each bar represents one `cpu_sample_secs` sample. |

### `[currency]`

Currency converter configuration. Exactly two currency codes are expected.

```toml
[currency]
units = ["USD", "EUR"]
```

| Field | Type | Default | Description |
|---|---|---|---|
| `units` | array of strings | `["USD", "EUR"]` | Two ISO 4217 currency codes for the converter panel |

### `[github]`

GitHub activity tracker configuration. This section is optional and defaults are used if omitted.

```toml
[github]
username = "Discordanian"
```

| Field | Type | Default | Description |
|---|---|---|---|
| `username` | string | `"Discordanian"` | GitHub username whose public contribution graph is displayed |

## Data Sources

| Panel | Source | Auth Required |
|---|---|---|
| Weather | [Open-Meteo API](https://open-meteo.com) | No |
| URL Status | Direct HTTP GET to each configured URL | No |
| IP / City | [ipinfo.io](https://ipinfo.io) | No |
| Currency | Public exchange rate API | No |
| GitHub | GitHub public contribution page (`github.com/users/{name}/contributions`) | No |
| VPN | Local network interface detection (tun/tap/utun/wg/ppp) | No |
| System | `sysinfo` crate (CPU, RAM) | No |
