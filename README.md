<div align="center">

<img src="src-tauri/icons/128x128.png" width="72" height="72" alt="Patina icon">

# Patina Linux Fork

Linux port and local AI/API integration fork of Patina.

English · [简体中文](README.zh-CN.md)

![Platform](https://img.shields.io/badge/platform-Linux%20prototype%20%7C%20Windows%20upstream-4f6f8f)
![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%20v2-4f7f8f)
![Local first](https://img.shields.io/badge/data-local--first-5f7f68)
![Status](https://img.shields.io/badge/status-fork%20prototype-b07a3a)
[![License](https://img.shields.io/badge/license-MIT-6f647a)](LICENSE)

</div>

![Patina dashboard](.github/assets/readme/dashboard.png)


This fork tracks the Linux migration work for Patina. The upstream project is a local-first Windows desktop time tracker; this fork is currently focused on GNOME/Linux foreground tracking, browser webpage activity, and a localhost API/MCP surface for external AI analysis.

The Linux port is usable as a development prototype, but it is not release-stable yet.

## Current Fork Focus

- GNOME Wayland foreground-window tracking through a GNOME Shell extension and session D-Bus.
- Linux X11 fallback path where available.
- Local HTTP API for external scripts, agents, and MCP wrappers.
- Browser activity sync through local browser extensions.
- Chromium extension support.
- Firefox/Zen extension support.
- Settings diagnostics for window tracking, local API, browser bridge, and Linux autostart.
- Repair action for Linux `~/.config/autostart/Patina.desktop`.

## Interface Preview
|  |  |
| --- | --- |
| **Today**<br>![Today page](.github/assets/readme/dashboard.png) | **History**<br>![History page](.github/assets/readme/history.png)|
| **Data**<br>![Data page](.github/assets/readme/data.png) | **Apps**<br>![Apps page](.github/assets/readme/mapping.png) |
| **Settings**<br>![Settings page](.github/assets/readme/settings.png) | **About**<br>![About page](.github/assets/readme/about.png) |

## Current Linux Status

| Area | Status | Notes |
|---|---|---|
| GNOME Wayland window tracking | Working prototype | Uses `org.patina.WindowTracker` from the GNOME Shell extension. |
| X11 tracking | Planned / partial | Kept as fallback direction, not the main verified path yet. |
| KDE / wlroots Wayland | Not promised | Needs compositor-specific work later. |
| Local API | Implemented | Binds to `127.0.0.1:14840` and uses a bearer token. |
| MCP wrapper | Prototype implemented | `npm run mcp:patina`. |
| Chromium Web Sync | Implemented | `extensions/chromium`. |
| Firefox / Zen Web Sync | Prototype implemented | `extensions/firefox`; signed XPI distribution is still manual. |
| Linux packaging | Not complete | Development workflow first, installer flow later. |

## Quick Start On Linux

### 1. Install dependencies

```bash
npm install
```

You also need Rust and the Tauri Linux build dependencies for your distribution.

### 2. Install the GNOME Shell extension

```bash
npm run extension:gnome:check
npm run extension:gnome:install
gnome-extensions enable patina-window-tracker@patina
```

Log out and back in if GNOME Shell keeps an older extension copy.

Verify D-Bus:

```bash
gdbus call --session \
  --dest org.patina.WindowTracker \
  --object-path /org/patina/WindowTracker \
  --method org.patina.WindowTracker.GetFocusedWindow
```

### 3. Run Patina

```bash
npm run tauri dev
```

The app starts the local API automatically:

```text
http://127.0.0.1:14840
```

Token path:

```text
${XDG_DATA_HOME:-~/.local/share}/Patina/api_token
```

## Browser Web Sync

Patina can record the active webpage URL/title/domain through a browser extension. It does not read page contents, form contents, screenshots, clipboard data, or browser history.

### Chromium / Chrome / Edge

Use:

```text
extensions/chromium
```

Open `chrome://extensions`, enable developer mode, and load the unpacked extension directory.

### Firefox / Zen

Use:

```text
extensions/firefox
```

For temporary development loading, open:

```text
about:debugging#/runtime/this-firefox
```

Then load `extensions/firefox/manifest.json`.

For persistent Firefox/Zen installation, install the signed `.xpi` package.

If you rebuild the extension locally, use `extensions/firefox/package.sh` to create an unsigned package first, then sign it before distribution.

## Local API

Set helper environment variables:

```bash
export PATINA_API_BASE="http://127.0.0.1:14840"
export PATINA_API_TOKEN="$(cat "${XDG_DATA_HOME:-$HOME/.local/share}/Patina/api_token")"
```

Example:

```bash
curl -s "$PATINA_API_BASE/api/v1/diagnostics" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Main endpoint index:

- [docs/api-index.md](docs/api-index.md)

Implemented API groups include diagnostics, current activity, sessions, summaries, trends, web activity, apps, and tracker settings.

## MCP Wrapper

The prototype MCP wrapper maps MCP tool calls to the local API:

```bash
npm run mcp:patina
```

It reads:

- `PATINA_API_BASE`
- `PATINA_API_TOKEN`
- `PATINA_API_TOKEN_FILE`

Current tools include:

- `get_diagnostics`
- `get_current_activity`
- `query_sessions`
- `get_active_session`
- `get_today_summary`
- `get_week_summary`
- `get_activity_trend`
- `query_web_activity`
- `list_apps`
- `classify_app`

## Useful Checks

```bash
npm run build
npm run test:settings
npm run extension:gnome:check
npm run extension:chromium:check
npm run extension:firefox:check
node --experimental-strip-types --experimental-specifier-resolution=node tests/patinaMcpScript.test.ts
cargo check --manifest-path src-tauri/Cargo.toml --quiet
```

## Upstream Product Background

Patina is a personal, local-first desktop time tracker. It automatically records foreground apps, handles AFK/lock/sleep/crash boundaries, stores data locally in SQLite, and provides dashboard/history/data/app-management views.

Upstream stable direction remains Windows-first. This fork explores whether the same product can become useful on Linux while also exposing enough structured local data for external AI analysis.

## Documentation

- Linux setup: [docs/linux-development-setup.md](docs/linux-development-setup.md)
- API index: [docs/api-index.md](docs/api-index.md)
- Linux/API design notes: [docs/linux-port-and-api-design.md](docs/linux-port-and-api-design.md)
- Product scope: [docs/product-principles-and-scope.md](docs/product-principles-and-scope.md)
- Architecture rules: [docs/architecture.md](docs/architecture.md)

## License

[MIT](LICENSE)
