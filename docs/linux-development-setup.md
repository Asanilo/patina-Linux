# Linux Development Setup

This page records the current Linux prototype setup path.

## GNOME Wayland Window Tracking

Patina uses a GNOME Shell extension on GNOME Wayland to read the focused window through session D-Bus.

Source:

```text
extensions/gnome-shell/patina-window-tracker@patina/
```

Check and build:

```bash
npm run extension:gnome:check
npm run extension:gnome:build
```

Install into the current user's GNOME Shell extension directory:

```bash
npm run extension:gnome:install
gnome-extensions enable patina-window-tracker@patina
```

If GNOME Shell has cached an older copy, log out and back in.

Verify the D-Bus endpoint:

```bash
gdbus call --session \
  --dest org.patina.WindowTracker \
  --object-path /org/patina/WindowTracker \
  --method org.patina.WindowTracker.GetFocusedWindow
```

Expected behavior:

- On GNOME Wayland, Patina should use `org.patina.WindowTracker`.
- On X11, Patina should use the X11 fallback path.
- KDE and wlroots Wayland are not currently promised.

## Local API

Patina starts a localhost API for external tools:

```text
http://127.0.0.1:14840
```

Token path:

```text
${XDG_DATA_HOME:-~/.local/share}/Patina/api_token
```

Example:

```bash
export PATINA_API_BASE="http://127.0.0.1:14840"
export PATINA_API_TOKEN="$(cat "${XDG_DATA_HOME:-$HOME/.local/share}/Patina/api_token")"

curl -s "$PATINA_API_BASE/api/v1/diagnostics" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

## MCP Wrapper

The prototype MCP wrapper is a stdio server that maps MCP tool calls to the local API:

```bash
npm run mcp:patina
```

It reads:

- `PATINA_API_BASE`
- `PATINA_API_TOKEN`
- `PATINA_API_TOKEN_FILE`

Current tools:

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

## Current Validation Commands

```bash
node --experimental-strip-types --experimental-specifier-resolution=node tests/gnomeShellExtensionScript.test.ts
node --experimental-strip-types --experimental-specifier-resolution=node tests/patinaMcpScript.test.ts
npm run extension:gnome:check
npm run extension:gnome:build
npm run build
cargo check --manifest-path src-tauri/Cargo.toml --quiet
```
