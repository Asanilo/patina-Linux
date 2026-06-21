# Patina Local API Index

> Status: active index for the local HTTP API prototype.
> Purpose: track what endpoints exist, their current scope, and known gaps for external AI/MCP integration.

---

## 1. Runtime

- Base URL: `http://127.0.0.1:14840`
- Protocol: local HTTP/1.1 JSON
- Auth: `Authorization: Bearer <token>`
- Token file: `${XDG_DATA_HOME:-~/.local/share}/Patina/api_token`
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

- API token and port do not have UI management yet.
- The server binds to localhost only.
- CORS is permissive for local integration.
- This is not an OpenAPI spec yet; it is the source-of-truth endpoint index while the API is still moving.

---

## 2. Implemented Endpoints

| Endpoint | Method | Status | Purpose |
|---|---:|---|---|
| `/api/v1/health` | `GET` | Implemented | API health, app version, platform |
| `/api/v1/diagnostics` | `GET` | Implemented | Platform, tracker runtime, and browser bridge diagnostics |
| `/api/v1/current` | `GET` | Implemented | Current foreground window snapshot |
| `/api/v1/sessions` | `GET` | Implemented | Closed session query |
| `/api/v1/sessions/active` | `GET` | Implemented | Current active session with realtime duration |
| `/api/v1/summary/today` | `GET` | Implemented | Local-day summary |
| `/api/v1/summary/range` | `GET` | Implemented | Caller-provided millisecond range summary |
| `/api/v1/summary/week` | `GET` | Implemented | Local-week summary |
| `/api/v1/trend` | `GET` | Partial | Daily activity trend for week/month |
| `/api/v1/web-activity` | `GET` | Implemented | Browser activity segment query |
| `/api/v1/apps` | `GET` | Implemented | Known apps from recorded sessions |
| `/api/v1/apps/{exe_name}/classify` | `POST` | Implemented | Save app category |
| `/api/v1/apps/{exe_name}/rename` | `POST` | Implemented | Save app display name |
| `/api/v1/apps/{exe_name}/exclude` | `POST` | Implemented | Save app exclusion flag |
| `/api/v1/settings/tracker` | `GET` | Implemented | Tracker settings snapshot |
| `/api/v1/settings/tracker/afk-threshold` | `POST` | Implemented | Update idle timeout threshold |

---

## 3. Endpoint Notes

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
- Aggregates closed sessions only.

Known gap:

- Active session is not yet included, so it can differ from frontend live read models.

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
- Aggregates closed sessions only.
- Filters by `start_time`; it does not clip sessions crossing range boundaries.

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
- Aggregates closed sessions only.

Known gap:

- Active session is not yet included.

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

- Stores and returns full `http` / `https` URLs for non-incognito tabs.
- Ignores non-web schemes such as `chrome://`.
- Active browser segment has `end_time: null`; `duration` is computed from the current API sample time.
- Domain-level recording overrides still apply at capture time.

Known gaps:

- No URL redaction mode in API settings yet.
- Query params are currently simple key/value parsing, not full URL-decoding.

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

| Endpoint | Method | Status | Purpose |
|---|---:|---|---|
| `/api/v1/tools/reminders` | `GET` | Not implemented | Query reminder state |
| `/api/v1/tools/reminders` | `POST` | Not implemented | Create reminder |
| `/api/v1/tools/reminders/{id}` | `DELETE` | Not implemented | Delete reminder |
| `/api/v1/tools/pomodoro` | `GET` | Not implemented | Query pomodoro state |
| `/api/v1/tools/pomodoro/start` | `POST` | Not implemented | Start pomodoro |
| `/api/v1/tools/pomodoro/stop` | `POST` | Not implemented | Stop pomodoro |

---

## 5. External AI/MCP Direction

The API is intended as the stable external integration surface.

Prototype MCP wrapper:

```bash
npm run mcp:patina
```

The wrapper is a dependency-free stdio MCP server that reads:

- `PATINA_API_BASE`, defaulting to `http://127.0.0.1:14840`
- `PATINA_API_TOKEN`, or `PATINA_API_TOKEN_FILE`, or the default token file path

Current MCP tools map to these endpoint groups:

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

Implemented in the prototype wrapper:

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

Remaining MCP wrapper gaps:

- `/api/v1/tools/*`
