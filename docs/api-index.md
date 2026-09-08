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
- The desktop runtime exposes the shared JSON endpoints below. Default `patinad` mode exposes authenticated reads plus SSE and rejects all `POST` endpoints. Explicit `--track` mode is the current runtime owner and additionally exposes the bounded activity-import, scheduled-backup, remote-backup upload, backup-restore, app-mapping, app-settings, classification, data-maintenance, local-API, runtime, service, Tools, and tracker writes listed by `/api/v1/capabilities`.
- Default daemon mode remains historical/read-only: `GET /api/v1/current` returns `503` and live tracker/browser diagnostics are `null`.
- Stage 2H.2 preview mode is explicit: run `patinad --profile dev --serve-api --track --port 0`. It owns tracking, Tools, and the local API listener for that profile, serves a live `/current`, observes Linux lock/suspend/resume/shutdown, runs audio/MPRIS participation sources, and owns the browser activity bridge configured for that profile. Never run desktop and daemon tracking against the same profile.
- Stage 2F capability migration, Stage 2F.1 browser crash/heartbeat semantics, and Stage 2F.2 loopback transport migration are complete. API, SSE, and the independent browser extension bridge use Axum with 32/8/8 fail-fast concurrency budgets, bounded handlers, strict Host/origin policies, and task-coupled listener readiness. The extension protocol remains `POST /web-activity` with its separate Token; its CORS response only echoes Firefox/Zen or Chromium extension origins and never returns `Access-Control-Allow-Origin: *`.
- The tracking-owner daemon can apply audio participation and the complete browser bridge configuration while running. Browser port changes reserve the new listener and commit storage before the old listener is stopped; bind or persistence failures preserve the old configuration.
- The tracking-owner daemon also owns local API port and credential changes. Local API port changes use the same reserve/commit/swap order. Token rotation updates the owner-only file atomically, revokes the old bearer value, and closes existing SSE authentication sessions without returning the new Token in JSON.
- A DEB can install `patinad.service` without enabling it. Only a daemon actually launched by that unit advertises the `service-lifecycle` and `backup-restore` scopes. Controlled restart returns a persistent `pending` ticket with HTTP `202`; the next systemd-managed instance changes the same ticket to `completed`.
- Controlled restore never accepts a local path or archive body over HTTP. Patina Desktop previews the archive, copies the unchanged bytes into the current profile's owner-only staging directory, and sends only a random ticket, SHA-256, size, strategy, and explicit confirmation. The new systemd-managed daemon restores before starting tracking or other background tasks.
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
| `/api/v1/apps` | `GET` | Implemented | Known apps from native and imported facts |
| `/api/v1/apps/{exe_name}/classify` | `POST` | Implemented | Save app category |
| `/api/v1/apps/{exe_name}/rename` | `POST` | Implemented | Save app display name |
| `/api/v1/apps/{exe_name}/exclude` | `POST` | Implemented | Save app exclusion flag |
| `/api/v1/imports` | `GET` | Implemented | List canonical activity import batches |
| `/api/v1/imports/canonical/commit` | `POST` | Tracking daemon | Consume a Desktop-created owner-only staging ticket and commit the revalidated CSV |
| `/api/v1/imports/{batch_id}/delete` | `POST` | Tracking daemon | Explicitly confirm deletion of one imported activity batch |
| `/api/v1/backups/schedule` | `GET` | Tracking daemon | Read the local scheduled-backup configuration and latest run state |
| `/api/v1/backups/schedule` | `POST` | Tracking daemon | Explicitly confirm and replace the daemon-owned local backup schedule |
| `/api/v1/backups/remote/upload` | `POST` | Tracking daemon | Create a database snapshot and upload it to a confirmed WebDAV target using the profile keyring credential |
| `/api/v1/backups/restore` | `GET` | Tracking daemon | Read the latest or specified controlled restore reservation |
| `/api/v1/backups/restore` | `POST` | Managed tracking daemon | Validate a staged archive, reserve startup restore, and request a controlled restart |
| `/api/v1/backups/restore/cancel` | `POST` | Managed tracking daemon | Explicitly cancel one failed restore reservation and remove its exact staged archive |
| `/api/v1/settings/tracker` | `GET` | Implemented | Tracker settings snapshot |
| `/api/v1/settings/tracker/afk-threshold` | `POST` | Implemented | Update idle timeout threshold |
| `/api/v1/settings/tracker/pause` | `POST` | Implemented | Set tracking pause state |
| `/api/v1/settings/classification` | `POST` | Implemented | Commit a validated classification mutation batch |
| `/api/v1/settings/app` | `POST` | Tracking daemon | Commit a validated non-resource app settings batch |
| `/api/v1/settings/runtime` | `GET` | Implemented | Sanitized audio and browser activity runtime settings |
| `/api/v1/settings/runtime/audio-participation` | `POST` | Tracking daemon | Apply and persist the Linux audio participation switch |
| `/api/v1/settings/runtime/browser-activity` | `POST` | Tracking daemon | Atomically replace browser listener, Token, and URL privacy settings |
| `/api/v1/settings/local-api` | `GET` | Tracking daemon | Read sanitized local API listener and credential-file state |
| `/api/v1/settings/local-api/port` | `POST` | Tracking daemon | Atomically move the local API listener |
| `/api/v1/settings/local-api/token/rotate` | `POST` | Tracking daemon | Rotate the owner-only API Token and revoke old clients |
| `/api/v1/data/cleanup` | `POST` | Tracking daemon | Delete tracking rows starting before an explicitly confirmed cutoff |
| `/api/v1/data/window-titles/clear` | `POST` | Tracking daemon | Explicitly confirm deletion and redaction of stored window titles |
| `/api/v1/data/apps/delete` | `POST` | Tracking daemon | Explicitly confirm deletion of native and imported activity for selected executables |
| `/api/v1/system/service` | `GET` | Managed tracking daemon | Read systemd service identity and latest restart ticket |
| `/api/v1/system/service/restart` | `POST` | Managed tracking daemon | Persist a restart ticket and gracefully return control to systemd |
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
- Request bodies: classify, rename, exclude, AFK threshold, tracking pause, classification/app-settings batches, audio participation, complete browser runtime configuration, confirmed scheduled-backup configuration, confirmed remote-backup upload, controlled backup restore scheduling/cancellation, confirmed service restart, reminders, timers, software reminders, and pomodoro writes
- Responses: success envelopes and standard `400` / `401` / `403` / `404` / `409` / `413` / `500` / `503` error envelopes
- Components: field-level schemas for health, capabilities, all runtime event variants, diagnostics, current window, sessions, active session, summaries, trend, web activity, apps, imports, scheduled backups and restore reservations, tracker/runtime settings, AI activity context, Tools snapshots, alerts, and Tools write requests

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
    "protocol_version": 2,
    "protocol": {
      "current": 2,
      "min_supported_client": 1,
      "max_supported_client": 2
    },
    "runtime_host": "daemon",
    "event_stream": { "available": true },
    "tracking": { "owned": false, "ready": false },
    "browser_activity_bridge": { "owned": false, "ready": false },
    "tools": { "owned": false, "ready": false },
    "daemon_service": { "owned": false, "ready": false },
    "write_api": { "available": false, "operations": [] }
  }
}
```

`owned` means that host is responsible for running the capability. `ready` is never true when `owned` is false. This prevents clients from confusing a readable historical API with a live tracking owner.

Protocol 2 adds the complete `runtime_snapshot` contract to `/api/v1/current`. Protocol 1 HTTP clients remain compatible with the existing flat fields, but desktop daemon clients require protocol 2 so they never reconstruct live state from incomplete data.

Clients compare their supported protocol against `protocol.min_supported_client` and `protocol.max_supported_client` before using the daemon. `protocol_version` remains as the compatibility alias for `protocol.current`.

With `--track`, the same response changes `tracking` to `{ "owned": true, "ready": false }` during startup and `{ "owned": true, "ready": true }` after the first runtime snapshot. `browser_activity_bridge.owned` is also `true`; its current `ready` value follows the configured listener task. `tools.owned` is `true` and becomes ready only after startup recovery and the first Tools snapshot. `write_api` includes every daemon-owned scope advertised by the current OpenAPI surface, including `activity-import`; clients must still satisfy endpoint-specific staging or confirmation rules. A process launched by `patinad.service` additionally reports `daemon_service` as owned/ready and includes `service-lifecycle`; a manually launched preview does not. Default daemon mode keeps all runtime capabilities unowned and the write API unavailable.

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

event: scheduled-backup-changed
data: {"sequence":3,"event":{"type":"scheduled-backup-changed","changed_at_ms":1782000001500}}

event: tool-alert
data: {"sequence":4,"event":{"type":"tool-alert","alert":{"id":"reminder:1","kind":"reminder","title":"提醒","body":"Review","occurred_at":1782000002000}}}
```

Behavior:

- Sequence IDs are monotonic within one daemon process and use a bounded in-memory replay window.
- Reconnect with `Last-Event-ID`; header names are case-insensitive and invalid IDs return `400`.
- `event: resync-required` means the cursor fell outside replay or the receiver lagged. Reload current/read-model snapshots through the JSON API.
- Daemon restart resets the sequence. Clients should call `/api/v1/capabilities` and reload snapshots after reconnect.
- Keepalive comments prevent idle local connections from being mistaken for a dead daemon.
- At most eight SSE streams are active at once. Additional streams fail immediately with `503` instead of creating unbounded long-lived tasks.
- Stage 2G/2H `--track` publishes real session transition, metadata, status, watchdog, runtime-shutdown, lock, suspend, system-shutdown, browser activity, scheduled-backup state-change, Tools snapshot-change, and Tools alert events. Audio and MPRIS participation affect tracking status through the same snapshots and events; default daemon mode still has no tracking, scheduled-backup, or Tools producer.

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
- `sampled_at_ms`: daemon 采集该窗口快照的时间；客户端可用它跳过更早的 SSE replay 事件
- `runtime_snapshot`: protocol 2 的完整 tracking runtime 状态，包括稳定窗口身份、追踪参与状态、probe 降级状态和恢复诊断；桌面 daemon client 应使用该对象，不应根据上面的兼容字段自行猜测状态

Schema:

```json
{
  "data": {
    "exe_name": "ghostty",
    "title": "patina",
    "process_id": 12345,
    "is_afk": false,
    "idle_time_ms": 2400,
    "process_path": "/usr/bin/ghostty",
    "sampled_at_ms": 1782000000000,
    "runtime_snapshot": {
      "window": {
        "hwnd": "0x100",
        "root_owner_hwnd": "0x100",
        "process_id": 12345,
        "window_class": "com.mitchellh.ghostty",
        "title": "patina",
        "exe_name": "ghostty",
        "process_path": "/usr/bin/ghostty",
        "is_afk": false,
        "idle_time_ms": 2400
      },
      "status": {
        "is_tracking_active": true,
        "sustained_participation_eligible": false,
        "sustained_participation_active": false,
        "sustained_participation_kind": null,
        "sustained_participation_state": "inactive",
        "sustained_participation_signal_source": null,
        "sustained_participation_reason": "not-eligible",
        "sustained_participation_diagnostics": {
          "state": "inactive",
          "reason": "not-eligible",
          "window_identity": null,
          "effective_signal_source": null,
          "last_match_at_ms": null,
          "grace_deadline_ms": null,
          "system_media": {
            "signal": {
              "is_available": false,
              "is_active": false,
              "signal_source": null,
              "source_app_id": null,
              "source_app_identity": null,
              "playback_type": null
            },
            "match_result": "unavailable"
          },
          "audio_session": {
            "signal": {
              "is_available": false,
              "is_active": false,
              "signal_source": null,
              "source_app_id": null,
              "source_app_identity": null,
              "playback_type": null
            },
            "match_result": "unavailable"
          }
        }
      },
      "sampled_at_ms": 1782000000000,
      "probe_status": "ok",
      "degraded_reason": null,
      "probe_diagnostics": {
        "last_successful_sample_at_ms": 1782000000000,
        "fallback_started_at_ms": null,
        "fallback_count": 0,
        "consecutive_fallback_count": 0,
        "recovery_attempt_count": 0,
        "last_recovery_attempt_at_ms": null
      }
    }
  }
}
```

字段级定义以 `/api/v1/openapi.json` 中的 `CurrentWindowResponse`、`TrackingRuntimeSnapshot`、`TrackingStatusSnapshot` 和相关 nested schema 为准。

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
- Returns native tracker sessions only. Imported exact records are not yet exposed through this endpoint.
- Never exposes imported hour buckets because they have no exact timeline position.

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
- Combines native sessions, imported exact sessions, and imported hour buckets using `native > import_exact > import_bucket` precedence.
- Includes the current native active session and clips exact facts to the local-day range.
- Treats hour buckets as aggregate quantities; a partial-hour query receives only its proportional share and never fabricates a timeline segment.
- Omits apps marked excluded; a local category override takes priority over an imported source category.

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
- Uses the same cross-source precedence as today/week summaries.
- Includes the current native active session and clips exact facts to both boundaries.
- Pro-rates aggregate-only hour buckets for partial bucket windows before applying remaining-capacity limits.
- Omits apps marked excluded.

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
- Uses the same cross-source precedence as today/range summaries.
- Includes the current native active session and clips exact facts to the local-week range.
- Omits apps marked excluded.

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
- Combines native sessions, imported exact sessions, and imported hour buckets with the same precedence as summary endpoints.
- Splits exact facts at local midnight and counts the active native session until current time.
- Keeps imported hour buckets aggregate-only.
- Omits apps marked excluded.
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

Returns apps discovered from native sessions and imported activity facts:

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

Executable names are merged case-insensitively. A native identity is preferred when the same app also appears in imported data.

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

### `GET /api/v1/imports`

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/imports" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Schema:

```json
{
  "data": [
    {
      "id": "import-<sha256>",
      "importedAt": 1788840000000,
      "sourceName": "activity.csv",
      "sourceKind": "patina-csv",
      "exactSessions": 12,
      "hourBuckets": 0,
      "totalRecords": 12
    }
  ]
}
```

### `POST /api/v1/imports/canonical/commit`

This endpoint is the database-owner half of the Desktop import flow. It does not accept a filesystem path or CSV body. Patina Desktop first copies the file into the current profile's owner-only staging directory and receives a random one-time `ticket`; `patinad` then consumes that ticket, enforces the 128 MiB limit, recomputes SHA-256, reparses the CSV, and commits the facts transactionally.

Illustrative request after Desktop has created the ticket:

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/imports/canonical/commit" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "ticket":"0123456789abcdef0123456789abcdef",
    "source_name":"activity.csv",
    "expected_fingerprint":"<64 lowercase hex characters>"
  }'
```

Schema:

```json
{
  "data": {
    "batchId": "import-<sha256>",
    "importedRecords": 12,
    "duplicateRecords": 2,
    "errorRecords": 0,
    "exactSessions": 12,
    "hourBuckets": 0
  }
}
```

The ticket is not a general file-access capability and is not intended for MCP tools. A missing/consumed ticket returns `404`, a fingerprint mismatch returns `409`, and invalid CSV/input returns `400`.

### `POST /api/v1/imports/{batch_id}/delete`

Curl:

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/imports/import-<sha256>/delete" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"confirmed":true}'
```

Schema:

```json
{
  "data": {
    "deletedExactSessions": 12,
    "deletedHourBuckets": 0
  }
}
```

Deletion is limited to one database-owned batch ID and requires `confirmed=true`.

### `GET /api/v1/backups/schedule`

This endpoint exists only when `patinad --track` owns the profile. It returns the persisted schedule and bounded run-state summary; it never starts a second scheduler in the Desktop process.

Curl:

```bash
curl -s "$PATINA_API_BASE/api/v1/backups/schedule" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Schema:

```json
{
  "data": {
    "config": {
      "enabled": true,
      "cadence": "daily",
      "weekday": null,
      "localTimeMinutes": 1260,
      "targetDir": "/home/user/Backups/Patina",
      "targetGeneration": "generation-id",
      "scheduleAnchorAtMs": 1788840000000,
      "updatedAtMs": 1788840000000
    },
    "nextExecutionAtMs": 1788926400000,
    "recentSuccess": null,
    "recentFailure": null,
    "activeRun": null
  }
}
```

Run status is one of `running`, `retry_wait`, `succeeded`, or `failed`. Exact run fields are defined by `ScheduledBackupRun` in OpenAPI.

### `POST /api/v1/backups/schedule`

This is a complete replacement operation. It requires `confirmed=true`, validates daily/weekly scheduling rules, normalizes the target directory, and wakes the daemon scheduler after persistence.

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/backups/schedule" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "config": {
      "enabled": true,
      "cadence": "weekly",
      "weekday": 7,
      "localTimeMinutes": 1260,
      "targetDir": "/home/user/Backups/Patina"
    },
    "confirmed": true
  }'
```

`weekday` uses `1` through `7` for Monday through Sunday and must be `null` for a daily schedule. The directory is a local filesystem capability selected by the user; this endpoint is intended for the trusted Desktop client and is deliberately not exposed as a generic MCP tool. Backup publication uses non-overwriting files and retention only removes verified, database-owned snapshots.

### `POST /api/v1/backups/remote/upload`

This tracking-daemon endpoint creates a consistent SQLite snapshot and uploads it to the explicitly confirmed WebDAV target. The request carries only non-secret target metadata; the daemon reads the password for its current profile from the operating-system credential store. On Linux that store is Secret Service, normally backed by GNOME Keyring or another compatible session service.

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/backups/remote/upload" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "config": {
      "url": "https://dav.example.test/remote.php/dav/files/user",
      "username": "user",
      "remoteDir": "/Patina"
    },
    "confirmed": true
  }'
```

Schema:

```json
{
  "data": {
    "entry": {
      "id": "20260908-140501-a1b2c3d4",
      "fileName": "Patina-backup-20260908-140501-a1b2c3d4.zip",
      "remotePath": "/Patina/Patina-backup-20260908-140501-a1b2c3d4.zip",
      "createdAtMs": 1788847501000,
      "sizeBytes": 1048576,
      "appVersion": "1.9.5",
      "backupVersion": 1,
      "schemaVersion": 16,
      "sessionCount": 120,
      "titleSampleCount": 80,
      "settingCount": 24,
      "iconCacheCount": 12
    },
    "indexUpdated": true,
    "indexMessage": null
  }
}
```

The daemon serializes remote backup operations, writes the temporary archive beneath the current profile's protected storage as a `0600` file, validates it through the normal backup reader, and removes that exact temporary file after the operation. A missing credential or unavailable keyring/WebDAV service returns `503`; invalid target input returns `400`. A successful archive upload can still return `indexUpdated=false` or a non-null `indexMessage` when the remote index or local completion timestamp could not be updated. The password is never accepted by this endpoint and is absent from logs, responses, and OpenAPI schemas.

This endpoint is for the trusted Desktop typed client and is intentionally absent from MCP. Remote listing and download still use the compatibility Desktop path in the current preview; they remain part of the next owner-migration batch.

### `GET /api/v1/backups/restore`

Returns the latest restore reservation, or the reservation selected by `request_id`. `data` is `null` when no reservation exists. The route belongs to the tracking-daemon surface; creating a reservation additionally requires a systemd-managed daemon.

```bash
curl -s "$PATINA_API_BASE/api/v1/backups/restore?request_id=restore_<32-hex>" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Schema:

```json
{
  "data": {
    "request_id": "restore_0123456789abcdef0123456789abcdef",
    "status": "completed",
    "strategy": "replace",
    "archive_sha256": "<64 lowercase hex>",
    "size_bytes": 1048576,
    "requested_at_ms": 1788840000000,
    "started_at_ms": 1788840001000,
    "completed_at_ms": 1788840001500,
    "restart_request_id": "restart_<id>",
    "error": null,
    "cleanup_warning": null
  }
}
```

`status` is one of `prepared`, `pending_restart`, `running`, `completed`, `failed`, or `cancelled`. Clients should keep the returned restore request ID, reconnect after the daemon restart, and poll until a terminal status is returned.

### `POST /api/v1/backups/restore`

Schedules a controlled startup restore and returns HTTP `202`. The request does not accept a path or archive body. `ticket` identifies an owner-only archive previously created by the trusted Desktop staging boundary; SHA-256 and size bind the request to those exact bytes.

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/backups/restore" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "ticket": "0123456789abcdef0123456789abcdef",
    "expected_sha256": "<64 lowercase hex>",
    "expected_size_bytes": 1048576,
    "strategy": "replace",
    "confirmed": true
  }'
```

The daemon revalidates the staged file, archive limits, checksums, and restore compatibility before persisting the reservation and requesting a systemd restart. The next instance performs the restore after SQLite migration but before API, tracking, browser, Tools, or other background tasks start. The database restore, scheduled-backup reset for `replace`, and idempotence receipt commit in one transaction. Active session, title, and web intervals are sealed to the backup's last trustworthy boundary rather than extended through downtime.

Host integration settings are intentionally not imported from the archive. The current local API port/legacy Token row, browser bridge port/Token, remote-status endpoint/Token/machine identity, and WebDAV target metadata remain bound to the current machine; ordinary UI, tracking, classification, and privacy preferences still follow Replace/Merge semantics. The owner-only API credential file and WebDAV password are never part of the archive.

This endpoint is intentionally absent from MCP, browser UI, CLI, and Agent tools. A manually launched preview daemon returns `409` because it cannot prove the restart handoff.

### `POST /api/v1/backups/restore/cancel`

Only a `failed` reservation can be cancelled. Cancellation requires the exact request ID and `confirmed=true`; it removes only the archive bound to that reservation and preserves the recorded failure reason for diagnostics.

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/backups/restore/cancel" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "request_id": "restore_0123456789abcdef0123456789abcdef",
    "confirmed": true
  }'
```

Validation or transaction failures leave the original database usable and retain the owner-only staged archive until this explicit cancellation. Successful restore records a durable receipt before the exact staged file is deleted, so a crash between database commit and status-file update cannot apply the same reservation twice.

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

### `POST /api/v1/settings/app`

This tracking-daemon endpoint exists primarily for trusted Patina clients. It accepts at most 256 mutations and commits the validated batch in one SQLite transaction.

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/settings/app" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"mutations":[{"key":"theme_mode","value":"dark"}]}'
```

Schema:

```json
{
  "mutations": [
    { "key": "theme_mode", "value": "dark" }
  ]
}
```

Only non-resource preferences such as appearance, language, timeline display, desktop behavior, startup preferences, and remote-status configuration are accepted. Tracker pause/AFK, audio, browser bridge, and local API settings are rejected here and must use their dedicated endpoints so live resources and persistence cannot diverge.

### `POST /api/v1/data/cleanup`

Deletes session title samples, native sessions, browser activity segments, imported exact sessions, and imported hour buckets whose owning row starts before `cutoff_time_ms`. Imported batch counts and empty batches are updated in the same SQLite transaction. A record crossing the cutoff is deleted according to its start time, matching the Settings cleanup policy.

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/data/cleanup" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"cutoff_time_ms":1786118400000,"confirmed":true}'
```

Schema:

```json
{
  "cutoff_time_ms": 1786118400000,
  "confirmed": true
}
```

The response reports `title_samples_deleted`, `sessions_deleted`, `web_activity_segments_deleted`, `imported_exact_sessions_deleted`, `imported_time_buckets_deleted`, and `import_batches_deleted`. A negative cutoff or missing/false confirmation returns `400` without writing.

### `POST /api/v1/data/window-titles/clear`

Deletes all title samples and replaces every non-empty native or imported exact-session window title with an empty string in one transaction.

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/data/window-titles/clear" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"confirmed":true}'
```

The response reports `title_samples_deleted`, `sessions_redacted`, and `imported_exact_sessions_redacted`. This clears existing data; it does not disable future title capture. Use per-app title recording controls for that policy. The endpoint is intentionally absent from the MCP wrapper.

### `POST /api/v1/data/apps/delete`

Deletes native sessions and imported exact/hour facts for one bounded executable set. The optional time range must provide both bounds and uses `[start_time_ms, end_time_ms)` against each row's start time. Imported batch counts and empty batches are updated in the same SQLite transaction.

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/data/apps/delete" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "exe_names":["code","code-insiders"],
    "start_time_ms":null,
    "end_time_ms":null,
    "confirmed":true
  }'
```

`exe_names` must contain 1 through 512 non-empty names, each at most 256 bytes. Missing/false confirmation, a partial range, a negative start, or `end_time_ms <= start_time_ms` returns `400` without writing. The response reports `sessions_deleted`, `imported_exact_sessions_deleted`, `imported_time_buckets_deleted`, and `import_batches_deleted`. Browser page history has a separate domain-scoped deletion flow and is not inferred from an executable name. This destructive endpoint is intentionally absent from MCP.

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

### `GET /api/v1/settings/local-api`

Available only when `/api/v1/capabilities` advertises the `local-api-configuration` write scope.

```bash
curl -s "$PATINA_API_BASE/api/v1/settings/local-api" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

```json
{
  "data": {
    "port": 14840,
    "base_url": "http://127.0.0.1:14840",
    "token_path": "/home/user/.local/share/Patina/api_token",
    "token_present": true
  }
}
```

The response never contains the Token value. `token_path` identifies the owner-only file for local MCP, CLI, and Agent clients.

### `POST /api/v1/settings/local-api/port`

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/settings/local-api/port" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"port":15555}'
```

The daemon binds the new loopback port before saving it, then switches the active listener and retires the old listener. A bind or persistence failure keeps the old listener and stored port. A successful response includes `configuration`, `previous_port`, and `reconnect_required`; clients must continue at `configuration.base_url`.

### `POST /api/v1/settings/local-api/token/rotate`

This is a security-sensitive operation. Confirm the user's intent before calling it.

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/settings/local-api/token/rotate" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"

PATINA_API_TOKEN="$(cat "$PATINA_API_TOKEN_FILE")"
```

The Token file is replaced atomically with owner-only permissions. The response reports `reauthentication_required: true` and sanitized configuration only. The old Token becomes invalid immediately, including for existing SSE streams; reconnect using the new file value. The browser extension Token is separate and is not changed.

### `GET /api/v1/system/service`

Available only on the tracking-daemon API surface. A manually launched preview reports `managed_by_systemd: false`; only an instance launched by the packaged user unit advertises the `service-lifecycle` write scope.

```bash
curl -s "$PATINA_API_BASE/api/v1/system/service" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

```json
{
  "data": {
    "service_name": "patinad.service",
    "managed_by_systemd": true,
    "instance_id": "instance_...",
    "restart": {
      "request_id": "restart_...",
      "status": "completed",
      "requested_at_ms": 1787528000000,
      "requested_instance_id": "instance_...",
      "completed_at_ms": 1787528002500,
      "completed_instance_id": "instance_..."
    }
  }
}
```

`restart` is `null` before the first request. Instance and request IDs are opaque identifiers, not credentials.

### `POST /api/v1/system/service/restart`

This is a lifecycle operation. Call it only after explicit user confirmation and only when capabilities include `service-lifecycle`.

```bash
curl -s -X POST "$PATINA_API_BASE/api/v1/system/service/restart" \
  -H "Authorization: Bearer $PATINA_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"confirmed":true}'
```

A successful request returns HTTP `202` with `reconnect_required: true` and a service snapshot whose restart status is `pending`. The daemon persists the ticket before replying, shuts down its runtime gracefully, and exits with the code expected by the unit's restart policy. After reconnecting, call `GET /api/v1/system/service` and verify that the same `request_id` is `completed`, with a different `completed_instance_id`. A manual daemon returns `409`; a second request while the current ticket is pending also returns `409`.

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

No additional endpoint path is committed. The next daemon migration work is client/service activation and owner cutover, not a new public route.

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
| `get_local_api_configuration` | `GET /api/v1/settings/local-api` | none | Read sanitized API connection state |
| `set_local_api_port` | `POST /api/v1/settings/local-api/port` | `port` | Atomically move the loopback listener |
| `rotate_local_api_token` | `POST /api/v1/settings/local-api/token/rotate` | `confirmed` must be `true` | Rotate credentials and revoke old clients |
| `get_daemon_service` | `GET /api/v1/system/service` | none | Read systemd ownership and the latest restart ticket |
| `restart_daemon_service` | `POST /api/v1/system/service/restart` | `confirmed` must be `true` | Request graceful restart and return a ticket for post-reconnect verification |
| `get_current_activity` | `GET /api/v1/current` | none | Read current foreground activity snapshot |
| `query_sessions` | `GET /api/v1/sessions` | `from`, `to`, `app`, `limit` | Query closed native sessions |
| `get_active_session` | `GET /api/v1/sessions/active` | none | Read the currently active session with realtime duration |
| `get_today_summary` | `GET /api/v1/summary/today` | none | Read cross-source local-day summary |
| `get_week_summary` | `GET /api/v1/summary/week` | none | Read cross-source local-week summary |
| `get_activity_trend` | `GET /api/v1/trend` | `period`, `granularity` | Read cross-source daily week/month trend |
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
| `list_apps` | `GET /api/v1/apps` | none | List apps from native and imported facts |
| `classify_app` | `POST /api/v1/apps/{exe_name}/classify` | `exeName`, `category` | Save an app category |
| `rename_app` | `POST /api/v1/apps/{exe_name}/rename` | `exeName`, `displayName` | Save an app display name |
| `set_app_excluded` | `POST /api/v1/apps/{exe_name}/exclude` | `exeName`, `excluded` | Save an app exclusion flag |

Remaining MCP wrapper gaps:

- Generated MCP tool metadata does not yet consume `/api/v1/openapi.json`; the wrapper keeps an explicit hand-written tool list for now.
