# tc-tui

Tangential Cold Dashboard — displays system info, weather, website health, currency conversion, and GitHub activity at a glance.

This repository is a Cargo **workspace** containing one shared, UI-agnostic core library plus four interchangeable frontends built on different Rust UI toolkits. Every frontend renders the exact same data and business logic, so you can compare the toolkits side by side.

## Workspace layout

```
tc-tui/
├── Cargo.toml            # workspace manifest + shared [workspace.dependencies]
├── config.toml           # reference configuration (copy to ~/.config/tc-tui/config.toml)
└── crates/
    ├── tc-core/          # UI-agnostic library: data, background fetchers, app state
    ├── tc-ratatui/       # terminal UI (ratatui)
    ├── tc-iced/          # GUI (iced — Elm-style / retained mode)
    ├── tc-egui/          # GUI (egui/eframe — immediate mode)
    └── tc-slint/         # GUI (slint — declarative .slint markup)
```

### `tc-core`

All data fetching and logic lives here, with **no dependency on any UI toolkit**:

- Background workers (URL health checks, weather, IP/city, currency rates, GitHub activity, VPN state, and a system CPU/RAM monitor) each run on their own thread and publish into shared state.
- `App` owns that shared state and the refresh handles. `App::snapshot()` returns a plain, clonable `Snapshot` view model.
- Every frontend follows the same three touchpoints: build the `App`, poll `snapshot()` on a timer, and forward refresh/input actions back to the core.

The unit tests for all parsing, formatting, and config logic live in `tc-core`.

## Prerequisites

- A recent stable [Rust toolchain](https://rustup.rs) (install via `rustup`).
- The GUI frontends (`tc-iced`, `tc-egui`, `tc-slint`) require a working graphics stack (they use `wgpu`/OpenGL via `winit`). On a headless machine, only `tc-ratatui` will run.

## Building

Build everything:

```bash
cargo build --workspace
```

Build a single frontend (only compiles that crate's dependency tree):

```bash
cargo build -p tc-ratatui
```

## Running

Each frontend is its own binary. Pick one with `-p`:

| Command | Frontend | UI style |
|---|---|---|
| `cargo run -p tc-ratatui` | ratatui | terminal |
| `cargo run -p tc-iced` | iced | Elm-style / retained |
| `cargo run -p tc-egui` | egui / eframe | immediate mode |
| `cargo run -p tc-slint` | slint | declarative markup |

For a smoother GUI experience, build in release mode:

```bash
cargo run -p tc-egui --release
```

## Testing

Run the whole suite (the meaningful tests live in `tc-core`):

```bash
cargo test --workspace
```

## Packaging

Examing two packaging systems
- cargo-bundle
- cargo-packager



## Configuration

All frontends read the same configuration file:

```
~/.config/tc-tui/config.toml
```

If the file is missing or cannot be parsed, built-in defaults are used, and each frontend shows which configuration is active. A reference configuration is included in the repository as `config.toml` — copy it to the path above and edit to taste. It controls weather locations, monitored URLs, refresh intervals, the CPU-history length, the currency pair, and the GitHub username/token.

## Keybindings (ratatui)

| Key | Action |
|---|---|
| `q` | Quit |
| `r` | Force-refresh all panels |
| `Tab` / `Up` / `Down` | Toggle the active currency input row |
| `0`–`9`, `.` | Type into the active currency input |
| `Backspace` | Delete the last character of the active currency input |

The GUI frontends expose a **Refresh** button instead of the `r` key, and their currency amount fields are directly editable.
