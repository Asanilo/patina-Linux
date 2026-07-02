<div align="center">

<img src="src-tauri/icons/128x128.png" width="72" height="72" alt="Patina icon">

# Patina Linux Fork

Linux port and local AI/API integration fork of Patina.

English · [简体中文](README.zh-CN.md)

![Platform](https://img.shields.io/badge/platform-Linux%20x86__64-4f6f8f)
![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%20v2-4f7f8f)
![Local first](https://img.shields.io/badge/data-local--first-5f7f68)
![Status](https://img.shields.io/badge/status-Linux--first%20development-b07a3a)
[![License](https://img.shields.io/badge/license-MIT-6f647a)](LICENSE)

</div>

![Patina dashboard](.github/assets/readme/dashboard.png)


This fork is the Linux-first edition of Patina. It focuses on GNOME/Linux foreground tracking, browser webpage activity, and a localhost API/MCP surface for external AI analysis. Windows platform sources remain as historical compatibility code, but they are outside the default CI, release pipeline, and current support commitment.

The Linux port is usable as a Linux-first desktop release. GNOME Wayland is the primary supported environment; KDE and wlroots compositors still need dedicated adapters.

## Current Fork Focus

- GNOME Wayland foreground-window tracking through a GNOME Shell extension and session D-Bus.
- Linux X11 fallback path where available.
- Local HTTP API for external scripts, agents, and MCP wrappers.
- Browser activity sync through local browser extensions.
- Chromium extension support.
- Firefox/Zen extension support.
- Settings diagnostics for window tracking, local API, browser bridge, and Linux autostart.
- Repair action for Linux `~/.config/autostart/Patina.desktop`.
- Stable custom categories with rename, merge, delete, and excluded-item filtering.
- HTTP and MCP Agent Skill guidance for external local analysis.

## Upstream Tracking Policy

The fork follows upstream Patina selectively: cross-platform UI, data, tracking-consistency, and quality fixes should be reviewed and ported when they fit the Linux-first product boundary. Platform-specific Windows release work is not copied into the Linux release line by default.

Already synced from upstream v1.8 work:

- History timeline zoom.
- History category distribution fix when Web Sync is disabled.
- Custom-category rename/merge and excluded filtering, restricted so built-in categories cannot be renamed or deleted.

Upstream-inspired work still needs Linux-specific design before porting:

- Local data directory and WebView cache management.
- Web Sync setup guide polish.
- Engineering cleanup around copy modules, quality hotspot checks, and bundle budget checks.

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
| X11 tracking | Implemented fallback / limited verification | Used on X11 sessions; GNOME Wayland does not silently fall back to X11. |
| KDE / wlroots Wayland | Not promised | Needs compositor-specific work later. |
| Local API | Implemented | Binds to `127.0.0.1:14840` and uses a bearer token. |
| MCP wrapper and Agent Skill | Implemented, query-first | `npm run mcp:patina`; write side currently covers app classify/rename/exclude, with HTTP and MCP skill references. |
| Chromium Web Sync | Implemented | `extensions/chromium`. |
| Firefox / Zen Web Sync | Implemented | The signed `0.1.1` XPI can be installed directly and identifies Firefox-family forks before generic Firefox. |
| Linux packaging | Release pipeline configured | Future version tags build x86_64 AppImage, `.deb`, browser/desktop extension assets, and a Linux-only updater manifest. |
| Local API token/port UI | Implemented | Settings applies ports atomically and rotates the owner-only API Token separately from browser Web Sync. |

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

## Linux Packages

The release workflow produces:

- `Patina_<version>_amd64.AppImage`
- `Patina_<version>_amd64.AppImage.tar.gz`
- `Patina_<version>_amd64.deb`
- `patina-gnome-shell-extension-v<version>.zip`
- `patina-chromium-extension-v<version>.zip`
- `patina-firefox-extension-v<version>.xpi`
- `latest.json`

Ubuntu and Debian users should prefer the `.deb`. It installs the GNOME Shell extension files into the system extension directory, but the extension must still be enabled for the current user:

```bash
gnome-extensions enable patina-window-tracker@patina
```

Log out and back in if GNOME Shell has cached an older extension.

The AppImage does not modify system directories:

```bash
chmod +x Patina_<version>_amd64.AppImage
./Patina_<version>_amd64.AppImage
```

GNOME Wayland users must also install the extension archive:

```bash
gnome-extensions install --force patina-gnome-shell-extension-v<version>.zip
gnome-extensions enable patina-window-tracker@patina
```

### Release Validation

Before publishing a Linux tag, run:

```bash
npm run release:validate-version-files -- <version>
npm run release:validate-changelog -- <version>
npm run test:release
npm run extension:gnome:check
npm run extension:chromium:check
npm run extension:firefox:check
```

`npm run test:release` verifies that the release workflow builds Linux-only bundles, that `prepare-linux-release-assets` copies the `.deb` into `dist-release`, and that `latest.json` points to the signed AppImage updater archive.

## Browser Web Sync

Patina can record the active webpage URL/title/domain through a browser extension. It does not read page contents, form contents, screenshots, clipboard data, or browser history.

The browser extension port/token shown in Settings are separate from the local API token used by `http://127.0.0.1:14840`.

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

If you rebuild the extension locally, use `extensions/firefox/package.sh` to create `dist/patina-web-sync-unsigned.xpi`, then sign it and replace `dist/patina-web-sync.xpi` before distribution. `npm run extension:firefox:verify-signed` checks the signature metadata, version, and packaged background source.

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

Implemented API groups include diagnostics, current activity, sessions, summaries, trends, web activity, apps, tracker settings, external AI context, and Tools snapshot. The local API also exposes `GET /api/v1/openapi.json` with a field-level OpenAPI 3.1 schema.

## MCP Wrapper

The MCP wrapper maps MCP tool calls to the local API:

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
- `get_activity_context`
- `get_tools_snapshot`
- `list_apps`
- `classify_app`
- `rename_app`
- `set_app_excluded`

See [docs/mcp-wrapper.md](docs/mcp-wrapper.md) for MCP client setup, tool-to-API mapping, and remaining write-side gaps.

For MCP client configuration, invoke `scripts/patina-mcp.ts` directly with Node and an absolute path. `npm run mcp:patina` is intended for manual repository use.

## Agent Skill

[`skills/analyzing-patina-activity`](skills/analyzing-patina-activity/SKILL.md) provides two external-agent workflows:

- MCP tools when the Patina MCP server is configured.
- Direct localhost HTTP with bearer authentication as a fallback.

Both workflows share rules for diagnostics-first analysis, local-time boundaries, active-session handling, URL privacy, and explicit confirmation before app-management writes.

## Useful Checks

```bash
npm run build
npm run test:release
npm run test:settings
npm run extension:gnome:check
npm run extension:chromium:check
npm run extension:firefox:check
node --experimental-strip-types --experimental-specifier-resolution=node tests/patinaMcpScript.test.ts
cargo check --manifest-path src-tauri/Cargo.toml --quiet
```

## Upstream Product Background

Patina is a personal, local-first desktop time tracker. It automatically records foreground apps, handles AFK/lock/sleep/crash boundaries, stores data locally in SQLite, and provides dashboard/history/data/app-management views.

This fork now treats Linux/GNOME as its product and release priority while exposing stable local structured data for external AI analysis. Other desktop platforms will only return to the support scope when they have a dedicated maintenance path.

## Documentation

- Linux setup: [docs/linux-development-setup.md](docs/linux-development-setup.md)
- API index: [docs/api-index.md](docs/api-index.md)
- Linux/API design notes: [docs/linux-port-and-api-design.md](docs/linux-port-and-api-design.md)
- Product scope: [docs/product-principles-and-scope.md](docs/product-principles-and-scope.md)
- Architecture rules: [docs/architecture.md](docs/architecture.md)

## License

[MIT](LICENSE)
