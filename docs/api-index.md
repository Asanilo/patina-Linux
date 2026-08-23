# Patina Local API Index

> Status: active reference for the local HTTP API.
> Purpose: track what endpoints exist, their current scope, and known gaps for external AI/MCP integration.

---

## 1. Runtime

- Base URL: `http://127.0.0.1:14840`
- Protocol: loopback HTTP JSON; `/api/v1/events` uses SSE.
- Auth: `Authorization: Bearer <token>`
- Production token file: `${XDG_DATA_HOME:-~/.local/share}/Patina/api_token`; Local and Dev use their matching profile directories.
- Token generation: created on first API startup if the token file is missing or empty.
- Response envelope: successful responses use `{ "data": ... }`.
- Error envelope: failed responses use `{ "error": { "code": "...", "message": "..." } }`.
- Timestamp unit: milliseconds since Unix epoch.

Shell helper:

```bash
export PATINA_API_BASE="http://127.0.0.1:14840"
export PATINA_API_TOKEN="$(cat "${XDG_DATA_HOME:-$HOME/.local/share}/Patina/api_token")"
```

All curl examples below assume those two variables are set.

Current caveats:

- API token and port can be managed from Settings.
- The server binds to localhost only.
- Requests without an `Origin` header, including curl, MCP, CLI, and local agents, are allowed after Bearer authentication. Requests with an `Origin` header are accepted only from loopback HTTP(S) or `tauri://localhost`; the server never returns `Access-Control-Allow-Origin: *`.
- The HTTP `Host` authority must be `localhost` or a loopback IP. API request bodies are limited to 64 KiB.
- `/api/v1/openapi.json` exposes the machine-readable OpenAPI 3.1 schema with paths, query/path parameters, request bodies, response envelopes, auth, error envelopes, and field-level component schemas.
- The OpenAPI server URL uses a configurable `{port}` variable whose default is `14840`.
- This document remains the human-maintained reference for behavior notes and implementation caveats.
- The desktop runtime exposes the shared JSON endpoints below. Default `patinad` mode exposes authenticated reads plus SSE and rejects all `POST` endpoints. Explicit `--track` mode is the current runtime owner and additionally exposes the bounded app-mapping, classification, tracker-settings, runtime-settings, and Tools writes listed by `/api/v1/capabilities`.
- Default daemon mode remains historical/read-only: `GET /api/v1/current` returns `503` and live tracker/browser diagnostics are `null`.
- Stage 2G preview mode is explicit: run `patinad --profile dev --serve-api --track --port 0`. It owns tracking and Tools for that profile, serves a live `/current`, observes Linux lock/suspend/resume/shutdown, runs audio/MPRIS participation sources, and owns the browser activity bridge configured for that profile. Never run desktop and daemon tracking against the same profile.
- Stage 2F capability migration, Stage 2F.1 browser crash/heartbeat semantics, and Stage 2F.2 loopback transport migration are complete. API, SSE, and the independent browser extension bridge use Axum with 32/8/8 fail-fast concurrency budgets, bounded handlers, strict Host/origin policies, and task-coupled listener readiness. The extension protocol remains `POST /web-activity` with its separate Token; its CORS response only echoes Firefox/Zen or Chromium extension origins and never returns `Access-Control-Allow-Origin: *`.
- The tracking-owner daemon can apply audio participation and the complete browser bridge configuration while running. Browser port changes reserve the new listener and commit storage before the old listener is stopped; bind or persistence failures preserve the old configuration.
- `/api/v1/events` accepts the token only through the `Authorization` header. It does not accept tokens in URLs or query strings.

---

## 2. Implemented Endpoints

| Endpoint | Method | Status | Purpose |
|---|---:|---|---|
| `/api/v1/health` | `GET` | Implemented | API health, app version, platform |
| `/api/v1/capabilities` | `GET` | Implemented | Runtime host, protocol, event stream, owner/readiness, and write-surface negotiation |
| `/api/v1/events` | `GET` | Daemon | Authenticated SSE runtime event stream with bounded replay |
| `/api/v1/openapi.json` | `GET` | Implemented | Machine-readable OpenAPI 3.1 schema |
| `/api/v1/diagnostics` | `GET` | Implemented | Platform, tracker runtime, and browser bridge diagnostics |
| `/api/v1/current` | `GET` | Implemented | Current foreground window snapshot |
| `/api/v1/sessions` | `GET` | Implemented | Closed session query |
| `/api/v1/sessions/active` | `GET` | Implemented | Current active session with realtime duration |
| `/api/v1/summary/today` | `GET` | Implemented | Local-day summary |
| `/api/v1/summary/range` | `GET` | Implemented | Caller-provided millisecond range summary |
| `/api/v1/summary/week` | `GET` | Implemented | Local-week summary |
| `/api/v1/trend` | `GET` | Partial | Daily activity trend for week/month |
| `/api/v1/web-activity` | `GET` | Implemented | Browser activity segment query |
| `/api/v1/ai/activity-context` | `GET` | Implemented | Aggregated diagnostics, active session, summaries, and recent web activity for external AI analysis |
| `/api/v1/apps` | `GET` | Implemented | Known apps from recorded sessions |
| `/api/v1/apps/{exe_name}/classify` | `POST` | Implemented | Save app category |
| `/api/v1/apps/{exe_name}/rename` | `POST` | Implemented | Save app display name |
| `/api/v1/apps/{exe_name}/exclude` | `POST` | Implemented | Save app exclusion flag |
| `/api/v1/settings/tracker` | `GET` | Implemented | Tracker settings snapshot |
| `/api/v1/settings/tracker/afk-threshold` | `POST` | Implemented | Update idle timeout threshold |
| `/api/v1/settings/tracker/pause` | `POST` | Implemented | Set tracking pause state |
| `/api/v1/settings/classification` | `POST` | Implemented | Commit a validated classification mutation batch |
| `/api/v1/settings/runtime` | `GET` | Implemented | Sanitized audio and browser activity runtime settings |
| `/api/v1/settings/runtime/audio-participation` | `POST` | Tracking daemon | Apply and persist the Linux audio participation switch |
| `/api/v1/settings/runtime/browser-activity` | `POST` | Tracking daemon | Atomically replace browser listener, Token, and URL privacy settings |
| `/api/v1/tools/snapshot` | `GET` | Implemented | Current Tools runtime snapshot |
| `/api/v1/tools/reminders` | `POST` | Tracking daemon | Create a scheduled reminder |
| `/api/v1/tools/reminders/{id}/cancel` | `POST` | Tracking daemon | Cancel a scheduled reminder |
| `/api/v1/tools/software-reminder-rules` | `POST` | Tracking daemon | Create a daily app usage reminder rule |
| `/api/v1/tools/software-reminder-rules/{id}/disable` | `POST` | Tracking daemon | Disable an app usage reminder rule |
| `/api/v1/tools/timer/start` | `POST` | Tracking daemon | Start stopwatch or countdown |
| `/api/v1/tools/timer/pause` | `POST` | Tracking daemon | Pause current timer |
| `/api/v1/tools/timer/resume` | `POST` | Tracking daemon | Resume current timer |
| `/api/v1/tools/timer/reset` | `POST` | Tracking daemon | Reset current timer |
| `/api/v1/tools/timer/laps` | `POST` | Tracking daemon | Add stopwatch lap |
| `/api/v1/tools/pomodoro/start` | `POST` | Tracking daemon | Start pomodoro run |
| `/api/v1/tools/pomodoro/pause` | `POST` | Tracking daemon | Pause pomodoro run |
| `/api/v1/tools/pomodoro/resume` | `POST` | Tracking daemon | Resume pomodoro run |
| `/api/v1/tools/pomodoro/skip` | `POST` | Tracking daemon | Skip current pomodoro phase |
| `/api/v1/tools/pomodoro/reset` | `POST` | Tracking daemon | Reset pomodoro run |

---

## 3. Endpoint Notes

### `GET /api/v1/openapi.json`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/openapi.json" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Current scope:

- OpenAPI version: `3.1.0`
- Auth model: bearer token through `components.securitySchemes.bearerAuth`
- Paths: the exact endpoints enabled for the current desktop or daemon API surface
- Parameters: query params for sessions, summary range, trend, web activity; path params for app management
- Request bodies: classify, rename, exclude, AFK threshold, tracking pause, classification batch, audio participation, complete browser runtime configuration, reminders, timers, software reminders, and pomodoro writes
- Responses: success envelopes and standard `400` / `401` / `403` / `404` / `409` / `413` / `500` / `503` error envelopes
- Components: field-level schemas for health, capabilities, all runtime event variants, diagnostics, current window, sessions, active session, summaries, trend, web activity, apps, tracker/runtime settings, AI activity context, Tools snapshots, alerts, and Tools write requests

### `GET /api/v1/health`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/health" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Returns:

- `status`
- `version`
- `platform`

Schema:

```json
{
  "data": {
    "status": "ok",
    "version": "0.0.0",
    "platform": "linux"
  }
}
```

### `GET /api/v1/capabilities`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/capabilities" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Default daemon schema:

```json
{
  "data": {
    "server_version": "1.8.3",
    "protocol_version": 1,
    "protocol": {
      "current": 1,
      "min_supported_client": 1,
      "max_supported_client": 1
    },
    "runtime_host": "daemon",
    "event_stream": { "available": true },
    "tracking": { "owned": false, "ready": false },
    "browser_activity_bridge": { "owned": false, "ready": false },
    "tools": { "owned": false, "ready": false },
    "write_api": { "available": false, "operations": [] }
  }
}
```

`owned` means that host is responsible for running the capability. `ready` is never true when `owned` is false. This prevents clients from confusing a readable historical API with a live tracking owner.

Clients compare their supported protocol against `protocol.min_supported_client` and `protocol.max_supported_client` before using the daemon. `protocol_version` remains as the compatibility alias for `protocol.current`.

With Stage 2G `--track`, the same response changes `tracking` to `{ "owned": true, "ready": false }` during startup and `{ "owned": true, "ready": true }` after the first runtime snapshot. `browser_activity_bridge.owned` is also `true`; its current `ready` value follows the configured listener task. `tools.owned` is `true` and becomes ready only after startup recovery and the first Tools snapshot. `write_api` becomes `{ "available": true, "operations": ["app-mapping", "classification", "runtime-settings", "tools", "tracker-settings"] }`. Default daemon mode keeps all runtime capabilities unowned and the write API unavailable.

### `GET /api/v1/events`

Daemon-only SSE connection:

```bash
curl -N "$PATINA_API_BASE/api/v1/events" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Last-Event-ID: 0"
```

Event frame:

```text
id: 1
event: tracking-data-changed
data: {"sequence":1,"event":{"type":"tracking-data-changed","reason":"window-changed","changed_at_ms":1782000000000}}
```

Other typed events use the same envelope:

```text
event: tools-runtime-changed
data: {"sequence":2,"event":{"type":"tools-runtime-changed","changed_at_ms":1782000001000}}

event: tool-alert
data: {"sequence":3,"event":{"type":"tool-alert","alert":{"id":"reminder:1","kind":"reminder","title":"提醒","body":"Review","occurred_at":1782000002000}}}
```

Behavior:

- Sequence IDs are monotonic within one daemon process and use a bounded in-memory replay window.
- Reconnect with `Last-Event-ID`; header names are case-insensitive and invalid IDs return `400`.
- `event: resync-required` means the cursor fell outside replay or the receiver lagged. Reload current/read-model snapshots through the JSON API.
- Daemon restart resets the sequence. Clients should call `/api/v1/capabilities` and reload snapshots after reconnect.
- Keepalive comments prevent idle local connections from being mistaken for a dead daemon.
- At most eight SSE streams are active at once. Additional streams fail immediately with `503` instead of creating unbounded long-lived tasks.
- Stage 2G `--track` publishes real session transition, metadata, status, watchdog, runtime-shutdown, lock, suspend, system-shutdown, browser activity, Tools snapshot-change, and Tools alert events. Audio and MPRIS participation affect tracking status through the same snapshots and events; default daemon mode still has no tracking or Tools producer.

### `GET /api/v1/diagnostics`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/diagnostics" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Returns:

- `window_tracking`: platform foreground-window capability state.
- `tracker_runtime`: latest tracker runtime probe state, or `null` if no runtime sample is available yet.
- `web_activity_bridge`: browser extension bridge state, or `null` if settings/storage are not ready.

Schema:

```json
{
  "data": {
    "window_tracking": {
      "status": "unavailable",
      "reason": "gnome-extension-dbus-unavailable",
      "provider": "gnome-shell-extension",
      "session_type": "wayland",
      "desktop": "GNOME"
    },
    "tracker_runtime": {
      "probe_status": "timeout-fallback",
      "degraded_reason": "probe-timeout",
      "probe_diagnostics": {
        "last_successful_sample_at_ms": 1782000000000,
        "fallback_started_at_ms": 1782000010000,
        "fallback_count": 3,
        "consecutive_fallback_count": 2,
        "recovery_attempt_count": 1,
        "last_recovery_attempt_at_ms": 1782000020000
      }
    },
    "web_activity_bridge": {
      "enabled": true,
      "listening": true,
      "connected": false,
      "browserClientId": "zen-profile",
      "browserKind": "firefox",
      "extensionVersion": "0.1.0",
      "lastActivityAtMs": 1782000030000
    }
  }
}
```

Current behavior:

- Does not fail the whole response when browser bridge settings/storage are unavailable; `web_activity_bridge` becomes `null`.
- On GNOME Wayland, `window_tracking.reason` distinguishes unavailable extension D-Bus from unsupported non-GNOME Wayland compositors.

### `GET /api/v1/current`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/current" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Returns the latest tracking runtime foreground window snapshot:

- `exe_name`
- `title`
- `process_id`
- `is_afk`
- `idle_time_ms`
- `process_path`

Schema:

```json
{
  "data": {
    "exe_name": "ghostty",
    "title": "patina",
    "process_id": 12345,
    "is_afk": false,
    "idle_time_ms": 2400,
    "process_path": "/usr/bin/ghostty"
  }
}
```

### `GET /api/v1/sessions`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/sessions?from=1782000000000&to=1782086400000&limit=50" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Query params:

- `from`: optional start timestamp in ms
- `to`: optional end timestamp in ms
- `app`: optional exact `exe_name`
- `limit`: optional row limit, defaults to `100`

Schema:

```json
{
  "data": {
    "sessions": [
      {
        "id": 1,
        "app_name": "Ghostty",
        "exe_name": "ghostty",
        "window_title": "patina",
        "start_time": 1782000000000,
        "end_time": 1782000300000,
        "duration": 300000
      }
    ]
  }
}
```

Current behavior:

- Returns closed sessions only.
- Filters by `start_time`; it does not clip sessions to the requested range.

### `GET /api/v1/sessions/active`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/sessions/active" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Returns `null` when no active session exists.

When active, returns:

- `id`
- `app_name`
- `exe_name`
- `window_title`
- `start_time`
- `end_time`: always `null`
- `duration`: realtime `sampled_at_ms - start_time`, clamped at `0`
- `continuity_group_start_time`
- `sampled_at_ms`

Schema:

```json
{
  "data": {
    "id": 42,
    "app_name": "Obsidian",
    "exe_name": "obsidian",
    "window_title": "Todo - Obsidian",
    "start_time": 1782000000000,
    "end_time": null,
    "duration": 180000,
    "continuity_group_start_time": 1781999900000,
    "sampled_at_ms": 1782000180000
  }
}
```

### `GET /api/v1/summary/today`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/summary/today" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Schema:

```json
{
  "data": {
    "date": "2026-06-21",
    "total_active_ms": 3600000,
    "apps": [
      {
        "exe_name": "ghostty",
        "total_ms": 1800000,
        "percentage": 50.0
      }
    ],
    "categories": [
      {
        "name": "Development",
        "total_ms": 1800000
      }
    ]
  }
}
```

Current behavior:

- Uses local day boundary.
- Includes closed sessions and the current active session.
- Clips every session to the local-day range before aggregation.

### `GET /api/v1/summary/range`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/summary/range?from=1782000000000&to=1782086400000" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Query params:

- `from`: required timestamp in ms
- `to`: required timestamp in ms

Schema:

Same response shape as `GET /api/v1/summary/today`.

Current behavior:

- Uses caller-provided millisecond bounds.
- Includes closed sessions and the current active session.
- Selects sessions that overlap the requested range and clips them to both boundaries.

### `GET /api/v1/summary/week`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/summary/week" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Schema:

Same response shape as `GET /api/v1/summary/today`; `date` is currently `"week"`.

Current behavior:

- Uses local week boundary, Monday start.
- Includes closed sessions and the current active session.
- Clips every session to the local-week range before aggregation.

### `GET /api/v1/trend`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/trend?period=week&granularity=day" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Query params:

- `period`: `week` or `month`, default `week`
- `granularity`: `day`, default `day`

Schema:

```json
{
  "data": {
    "period": "week",
    "granularity": "day",
    "from_ms": 1781481600000,
    "to_ms": 1782086400000,
    "data_points": [
      {
        "date": "2026-06-21",
        "active_ms": 3600000,
        "top_app": "ghostty"
      }
    ]
  }
}
```

Current behavior:

- Uses local date buckets.
- Splits cross-day sessions at local midnight.
- Counts active sessions until current time.
- Returns one point per day with:
  - `date`
  - `active_ms`
  - `top_app`

Known gaps:

- No `hour` or `week` granularity yet.
- No category trend yet.
- No explicit timezone field yet.

### `GET /api/v1/web-activity`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/web-activity?from=1782000000000&to=1782086400000&domain=github.com&limit=50" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Query params:

- `from`: optional lower timestamp in ms. Matches segments whose `end_time`, or current sampled time for active segments, is at or after this value.
- `to`: optional upper timestamp in ms. Matches segments whose `start_time` is at or before this value.
- `domain`: optional exact normalized domain filter, for example `github.com`.
- `limit`: optional row limit, defaults to `100`, clamped to `1..1000`.

Returns browser activity segments ordered by newest first:

- `id`
- `browser_client_id`
- `browser_kind`
- `browser_exe_name`
- `domain`
- `normalized_domain`
- `url`
- `title`
- `favicon_url`
- `start_time`
- `end_time`
- `duration`
- `source`

URL privacy:

- The Settings page controls how the `url` field is exposed to local API clients.
- `Full URL` keeps the captured URL unchanged.
- `Remove query and fragment` strips `?query` and `#fragment` before returning `url`.
- `Domain only` returns `url: null`; `domain` and `normalized_domain` remain available for grouping and AI aggregation.

Schema:

```json
{
  "data": {
    "items": [
      {
        "id": 7,
        "browser_client_id": "client",
        "browser_kind": "chrome",
        "browser_exe_name": "google-chrome",
        "domain": "github.com",
        "normalized_domain": "github.com",
        "url": "https://github.com/Ceceliaee/patina",
        "title": "Ceceliaee/patina",
        "favicon_url": "https://github.com/favicon.ico",
        "start_time": 1782000000000,
        "end_time": 1782000300000,
        "duration": 300000,
        "source": "browser-extension"
      }
    ]
  }
}
```

Current behavior:

- Stores `http` / `https` URLs for non-incognito tabs, then applies the configured URL privacy mode when serving this API.
- Ignores non-web schemes such as `chrome://`.
- Active browser segment has `end_time: null`; `duration` is computed from the current API sample time.
- Domain-level recording overrides still apply at capture time.

Known gaps:

- Query params are currently simple key/value parsing, not full URL-decoding.

### `GET /api/v1/ai/activity-context`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/ai/activity-context" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Returns one AI-oriented context bundle:

- `diagnostics`: same `data` shape as `GET /api/v1/diagnostics`.
- `active_session`: same nullable `data` shape as `GET /api/v1/sessions/active`.
- `today_summary`: same `data` shape as `GET /api/v1/summary/today`.
- `week_summary`: same `data` shape as `GET /api/v1/summary/week`.
- `recent_web_activity`: same `data` shape as `GET /api/v1/web-activity`, limited to the 25 newest segments.

Schema:

```json
{
  "data": {
    "diagnostics": {
      "window_tracking": {
        "status": "available",
        "reason": null,
        "provider": "gnome-shell-extension",
        "session_type": "wayland",
        "desktop": "GNOME"
      },
      "tracker_runtime": null,
      "web_activity_bridge": null
    },
    "active_session": null,
    "today_summary": {
      "date": "2026-06-27",
      "total_active_ms": 0,
      "apps": [],
      "categories": []
    },
    "week_summary": {
      "date": "week",
      "total_active_ms": 0,
      "apps": [],
      "categories": []
    },
    "recent_web_activity": {
      "items": []
    }
  }
}
```

Current behavior:

- Accepts no query parameters; use the component endpoints for custom ranges or filters.
- Applies the configured URL privacy mode to `recent_web_activity`.
- The outer response remains `200` when a component handler fails. The failed component is replaced with `{ "error": <component error envelope> }`; consumers must check each component before analysis.

### `GET /api/v1/apps`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/apps" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Returns apps discovered from recorded sessions:

- `exe_name`
- `display_name`
- `category`
- `excluded`

Schema:

```json
{
  "data": {
    "apps": [
      {
        "exe_name": "ghostty",
        "display_name": "Ghostty",
        "category": "Development",
        "excluded": false
      }
    ]
  }
}
```

Known gap:

- Only apps with session history appear.

### `POST /api/v1/apps/{exe_name}/classify`

Curl:

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/apps/ghostty/classify" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"category":"Development"}'
```

Body:

```json
{ "category": "Development" }
```

Schema:

```json
{ "data": { "ok": true } }
```

App mapping writes use one transactional `__app_override::<canonical exe_name>` record. Updating one field preserves the existing category, display name, color, exclusion and title-capture fields; old API-only category/exclusion keys are migrated during the write.

### `POST /api/v1/apps/{exe_name}/rename`

Curl:

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/apps/ghostty/rename" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"display_name":"Ghostty"}'
```

Body:

```json
{ "display_name": "Ghostty" }
```

Schema:

```json
{ "data": { "ok": true } }
```

### `POST /api/v1/apps/{exe_name}/exclude`

Curl:

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/apps/ghostty/exclude" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"excluded":true}'
```

Body:

```json
{ "excluded": true }
```

Schema:

```json
{ "data": { "ok": true } }
```

### `GET /api/v1/settings/tracker`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/settings/tracker" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Returns:

- `idle_timeout_secs`
- `timeline_merge_gap_secs`
- `tracking_paused`

Schema:

```json
{
  "data": {
    "idle_timeout_secs": 900,
    "timeline_merge_gap_secs": 30,
    "tracking_paused": false
  }
}
```

Known gap:

- Current API defaults must stay aligned with frontend/Rust startup defaults.

### `POST /api/v1/settings/tracker/afk-threshold`

Curl:

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/settings/tracker/afk-threshold" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"seconds":900}'
```

Body:

```json
{ "seconds": 900 }
```

Schema:

```json
{ "data": { "ok": true } }
```

Current behavior:

- Updates in-memory AFK threshold.
- Persists `idle_timeout_secs`.
- Accepts values from 60 through 86400 seconds.
- Returns an error if persistence fails instead of reporting a false success.

### `POST /api/v1/settings/tracker/pause`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/settings/tracker/pause" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"paused":true}'
```

Body:

```json
{ "paused": true }
```

The tracking owner observes this persisted state on its next loop, seals active activity when paused, and publishes a refresh event.

### `POST /api/v1/settings/classification`

This endpoint is for the desktop classification editor and other trusted local clients that already hold explicit user intent. It accepts at most 256 mutations and only classification key prefixes validated by the data owner.

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/settings/classification" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"mutations":[{"key":"__web_domain_override::example.com","value":"{\"category\":\"research\",\"enabled\":true}"}]}'
```

`value: null` deletes the key. Invalid keys or malformed override JSON reject the whole batch before any write; valid batches commit in one transaction.

### `GET /api/v1/settings/runtime`

```bash
curl -s "$PATINA_API_BASE/api/v1/settings/runtime" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Schema:

```json
{
  "data": {
    "audio_participation_enabled": true,
    "browser_activity": {
      "enabled": true,
      "port": 12345,
      "token_present": true,
      "url_privacy": "domain_only"
    }
  }
}
```

This endpoint is available on every API surface and reports persisted configuration. It deliberately exposes only `token_present`; the browser extension Token is never returned. Use diagnostics to distinguish configured state from whether a runtime owner is currently listening or connected.

### `POST /api/v1/settings/runtime/audio-participation`

Available only when `/api/v1/capabilities` advertises the `runtime-settings` write scope.

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/settings/runtime/audio-participation" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"enabled":false}'
```

The daemon persists the setting before changing the live Linux audio signal source. This controls only audio-based participation accuracy; Patina does not capture or store microphone audio.

### `POST /api/v1/settings/runtime/browser-activity`

This is a complete replacement operation. Confirm all four fields before calling it.

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/settings/runtime/browser-activity" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"enabled":true,"port":12345,"token":"replace-with-extension-token","url_privacy":"domain_only"}'
```

`port` must be `1024..65535`; `url_privacy` is `full`, `strip_query`, or `domain_only`; enabling requires a non-empty Token. For a port change, the daemon first binds the requested port, then commits all browser settings in one transaction, and only then replaces the old listener. A bind conflict returns `409` without changing stored or live settings. Disabling also seals any active web segment at the current trusted boundary. The response reports `token_present` and never echoes the Token value.

### `GET /api/v1/tools/snapshot`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/tools/snapshot" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Top-level fields:

| Field | Type | Meaning |
|---|---|---|
| `settings` | object | Default countdown and pomodoro durations |
| `reminders` | array | Scheduled, fired, or cancelled reminders |
| `software_reminder_rules` | array | Per-application daily usage reminder rules |
| `current_timer` | object or `null` | Current stopwatch/countdown state |
| `timer_laps` | array | Laps belonging to the current timer |
| `current_pomodoro` | object or `null` | Current pomodoro run and phase |
| `today_completed_pomodoros` | integer | Completed focus phases for the local day |
| `next_reminder_at` | integer or `null` | Next reminder timestamp in milliseconds |
| `sampled_at_ms` | integer | Snapshot sample timestamp in milliseconds |

Nested fields:

| Object | Fields |
|---|---|
| `settings` | `default_countdown_minutes`, `pomodoro_focus_minutes`, `pomodoro_short_break_minutes`, `pomodoro_long_break_minutes`, `pomodoro_long_break_every` |
| reminder | `id`, `label`, `scheduled_at`, `created_at`, `status`, `fired_at`, `cancelled_at` |
| software reminder rule | `id`, `app_name`, `exe_name`, `limit_ms`, `message`, `created_at`, `updated_at`, `disabled_at`, `last_fired_date_key` |
| timer | `id`, `mode`, `label`, `duration_ms`, `accumulated_ms`, `started_at`, `paused_at`, `completed_at`, `status`, `created_at`, `updated_at` |
| timer lap | `id`, `timer_id`, `lap_index`, `started_at`, `ended_at`, `duration_ms` |
| pomodoro | `id`, `phase`, `status`, `cycle_index`, `focus_ms`, `short_break_ms`, `long_break_ms`, `long_break_every`, `phase_started_at`, `phase_paused_at`, `phase_remaining_ms`, `completed_focus_count`, `created_at`, `updated_at` |

Enum values:

- Reminder `status`: `scheduled`, `fired`, `cancelled`.
- Timer `mode`: `stopwatch`, `countdown`.
- Timer and pomodoro `status`: `idle`, `running`, `paused`, `completed`.
- Pomodoro `phase`: `focus`, `short_break`, `long_break`.

Schema:

```json
{
  "data": {
    "settings": {
      "default_countdown_minutes": 25,
      "pomodoro_focus_minutes": 25,
      "pomodoro_short_break_minutes": 5,
      "pomodoro_long_break_minutes": 15,
      "pomodoro_long_break_every": 4
    },
    "reminders": [],
    "software_reminder_rules": [],
    "current_timer": null,
    "timer_laps": [],
    "current_pomodoro": null,
    "today_completed_pomodoros": 0,
    "next_reminder_at": null,
    "sampled_at_ms": 1782528000000
  }
}
```

Current behavior:

- This endpoint is read-only and available on all surfaces. Tools writes require a tracking-owner daemon whose capabilities include the `tools` scope.
- Every successful Tools write returns this same complete snapshot envelope.
- Live elapsed or remaining time must be interpreted relative to `sampled_at_ms` and the current object's timestamps.

The tracking daemon continues Tools ticks after the desktop window closes. It fires reminders, completes countdown/pomodoro boundaries, sends Linux desktop notifications, and publishes `tools-runtime-changed` or `tool-alert` through `/api/v1/events`.

### `POST /api/v1/tools/reminders`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/reminders" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"label":"Review notes","scheduled_at":1900000000000}'
```

`scheduled_at` must be a future epoch-millisecond timestamp. `label` may be empty and is limited to 256 characters.

### `POST /api/v1/tools/reminders/{id}/cancel`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/reminders/1/cancel" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

`id` must be a positive reminder ID. Cancelling an already completed or cancelled reminder is an idempotent no-op.

### `POST /api/v1/tools/software-reminder-rules`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/software-reminder-rules" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"app_name":"Zen","exe_name":"zen","limit_ms":3600000,"message":"Take a break"}'
```

`app_name` is required; `exe_name` may be `null`; `limit_ms` is `60000..86400000`; `message` is limited to 1024 characters. A rule fires at most once per local day.

### `POST /api/v1/tools/software-reminder-rules/{id}/disable`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/software-reminder-rules/1/disable" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

`id` must be a positive rule ID. Disabling an already disabled rule is an idempotent no-op.

### `POST /api/v1/tools/timer/start`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/timer/start" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"mode":"countdown","duration_ms":1500000,"label":"Focus"}'
```

`mode` is `stopwatch` or `countdown`. Countdown duration is required and limited to `60000..10800000`; stopwatch duration is ignored. `label` may be `null` and is limited to 256 characters.

### `POST /api/v1/tools/timer/pause`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/timer/pause" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

### `POST /api/v1/tools/timer/resume`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/timer/resume" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

### `POST /api/v1/tools/timer/reset`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/timer/reset" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

### `POST /api/v1/tools/timer/laps`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/timer/laps" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Pause, resume, reset, and lap are idempotent when the current timer state cannot perform the requested transition. A lap is added only to a running timer.

### `POST /api/v1/tools/pomodoro/start`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/pomodoro/start" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"focus_ms":1500000,"short_break_ms":300000,"long_break_ms":900000,"long_break_every":4}'
```

Focus duration is `60000..10800000`, short break is `60000..3600000`, long break is `60000..7200000`, and `long_break_every` is `2..12`.

### `POST /api/v1/tools/pomodoro/pause`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/pomodoro/pause" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

### `POST /api/v1/tools/pomodoro/resume`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/pomodoro/resume" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

### `POST /api/v1/tools/pomodoro/skip`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/pomodoro/skip" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

### `POST /api/v1/tools/pomodoro/reset`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/tools/pomodoro/reset" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Pomodoro transition actions are idempotent when no matching active state exists. Skip advances to the next phase and leaves that phase paused.

### Error responses

Unauthorized:

```bash
curl -s "$PATINA_API_BASE/api/v1/health"
```

```json
{
  "error": {
    "code": "unauthorized",
    "message": "Invalid or missing API token"
  }
}
```

Not found:

```bash
curl -s "$PATINA_API_BASE/api/v1/missing" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

```json
{
  "error": {
    "code": "not_found",
    "message": "endpoint not found"
  }
}
```

---

## 4. Planned Endpoints

No additional endpoint path is committed. API listener port/Token ownership and controlled service restart remain runtime-owner work; they will not receive public routes until their atomicity and authorization contract is designed.

---

## 5. External AI/MCP Direction

The API is intended as the stable external integration surface.

MCP wrapper:

```bash
npm run mcp:patina
```

Full MCP setup and client configuration are documented in [`mcp-wrapper.md`](./mcp-wrapper.md).

The wrapper is a dependency-free stdio MCP server that reads:

- `PATINA_API_BASE`, defaulting to `http://127.0.0.1:14840`
- `PATINA_API_TOKEN`, or `PATINA_API_TOKEN_FILE`, or the default token file path

Current MCP tools:

| Tool | HTTP API | Arguments | Purpose |
|---|---|---|---|
| `get_diagnostics` | `GET /api/v1/diagnostics` | none | Check Linux/window/browser/API runtime health |
| `get_runtime_settings` | `GET /api/v1/settings/runtime` | none | Read sanitized audio and browser activity settings |
| `get_current_activity` | `GET /api/v1/current` | none | Read current foreground activity snapshot |
| `query_sessions` | `GET /api/v1/sessions` | `from`, `to`, `app`, `limit` | Query closed activity sessions |
| `get_active_session` | `GET /api/v1/sessions/active` | none | Read the currently active session with realtime duration |
| `get_today_summary` | `GET /api/v1/summary/today` | none | Read local-day summary |
| `get_week_summary` | `GET /api/v1/summary/week` | none | Read local-week summary |
| `get_activity_trend` | `GET /api/v1/trend` | `period`, `granularity` | Read daily week/month trend |
| `query_web_activity` | `GET /api/v1/web-activity` | `from`, `to`, `domain`, `limit` | Query browser extension activity segments |
| `get_activity_context` | `GET /api/v1/ai/activity-context` | none | Fetch diagnostics, active session, summaries, and recent web activity for external AI analysis |
| `get_tools_snapshot` | `GET /api/v1/tools/snapshot` | none | Fetch current Tools runtime snapshot |
| `create_reminder` | `POST /api/v1/tools/reminders` | `label`, `scheduledAt` | Create a scheduled reminder |
| `cancel_reminder` | `POST /api/v1/tools/reminders/{id}/cancel` | `id` | Cancel a reminder |
| `create_software_reminder_rule` | `POST /api/v1/tools/software-reminder-rules` | `appName`, optional `exeName`, `limitMs`, `message` | Create a daily app usage reminder |
| `disable_software_reminder_rule` | `POST /api/v1/tools/software-reminder-rules/{id}/disable` | `id` | Disable an app usage reminder |
| `start_timer` | `POST /api/v1/tools/timer/start` | `mode`, optional `durationMs`, `label` | Start stopwatch or countdown |
| `pause_timer` | `POST /api/v1/tools/timer/pause` | none | Pause current timer |
| `resume_timer` | `POST /api/v1/tools/timer/resume` | none | Resume current timer |
| `reset_timer` | `POST /api/v1/tools/timer/reset` | none | Reset current timer |
| `add_timer_lap` | `POST /api/v1/tools/timer/laps` | none | Add stopwatch lap |
| `start_pomodoro` | `POST /api/v1/tools/pomodoro/start` | four duration/cycle fields | Start pomodoro run |
| `pause_pomodoro` | `POST /api/v1/tools/pomodoro/pause` | none | Pause pomodoro run |
| `resume_pomodoro` | `POST /api/v1/tools/pomodoro/resume` | none | Resume pomodoro run |
| `skip_pomodoro_phase` | `POST /api/v1/tools/pomodoro/skip` | none | Skip current phase |
| `reset_pomodoro` | `POST /api/v1/tools/pomodoro/reset` | none | Reset pomodoro run |
| `list_apps` | `GET /api/v1/apps` | none | List known apps from recorded sessions |
| `set_idle_threshold` | `POST /api/v1/settings/tracker/afk-threshold` | `seconds` | Set idle threshold |
| `set_tracking_paused` | `POST /api/v1/settings/tracker/pause` | `paused` | Set tracking pause state |
| `set_audio_participation` | `POST /api/v1/settings/runtime/audio-participation` | `enabled` | Apply the Linux audio participation switch |
| `configure_browser_activity` | `POST /api/v1/settings/runtime/browser-activity` | `enabled`, `port`, `token`, `urlPrivacy` | Replace browser activity runtime configuration |
| `classify_app` | `POST /api/v1/apps/{exe_name}/classify` | `exeName`, `category` | Save an app category |
| `rename_app` | `POST /api/v1/apps/{exe_name}/rename` | `exeName`, `displayName` | Save an app display name |
| `set_app_excluded` | `POST /api/v1/apps/{exe_name}/exclude` | `exeName`, `excluded` | Save an app exclusion flag |

Remaining MCP wrapper gaps:

- Local API configuration tools.
- Generated MCP tool metadata does not yet consume `/api/v1/openapi.json`; the wrapper keeps an explicit hand-written tool list for now.
