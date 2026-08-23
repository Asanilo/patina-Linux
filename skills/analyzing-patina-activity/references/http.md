# HTTP Workflow

Use direct HTTP when MCP is unavailable or when exact range aggregation through `/api/v1/summary/range` is required.

## Connection

```bash
export PATINA_API_BASE="http://127.0.0.1:14840"
export PATINA_API_TOKEN_FILE="${XDG_DATA_HOME:-$HOME/.local/share}/Patina/api_token"
export PATINA_API_TOKEN="$(cat "$PATINA_API_TOKEN_FILE")"
```

The port can differ when changed in Patina Settings. Keep the token in an environment variable; never place its value in prompts, reports, source files, or command output.

## Discover and Diagnose

Check health, then read the live machine-readable contract:

```bash
curl -fsS "$PATINA_API_BASE/api/v1/health" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"

curl -fsS "$PATINA_API_BASE/api/v1/openapi.json" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"

curl -fsS "$PATINA_API_BASE/api/v1/diagnostics" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Prefer the live OpenAPI schema over remembered fields. Successful responses use `{ "data": ... }`; failures use `{ "error": { "code", "message" } }`.
Read `/api/v1/capabilities` before writes. Default daemon mode is read-only; the tracking-owner daemon advertises the exact write scopes it supports.

## Analysis Queries

Overview:

```bash
curl -fsS "$PATINA_API_BASE/api/v1/ai/activity-context" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Exact clipped range:

```bash
curl -fsS "$PATINA_API_BASE/api/v1/summary/range?from=START_MS&to=END_MS" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Browser segments:

```bash
curl -fsS "$PATINA_API_BASE/api/v1/web-activity?from=START_MS&to=END_MS&limit=100" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Other stable read paths are `/current`, `/sessions`, `/sessions/active`, `/summary/today`, `/summary/week`, `/trend`, `/apps`, `/settings/tracker`, `/settings/runtime`, `/settings/local-api`, `/system/service`, and `/tools/snapshot`, all under `/api/v1`. Runtime and local API settings are sanitized: credentials are represented only by `token_present` plus an owner-only file path where applicable.

## Writes

Only perform writes on explicit user intent. Verify the exact app from `GET /api/v1/apps`, then use one of:

- `POST /api/v1/apps/{exe_name}/classify` with `{ "category": "..." }`.
- `POST /api/v1/apps/{exe_name}/rename` with `{ "display_name": "..." }`.
- `POST /api/v1/apps/{exe_name}/exclude` with `{ "excluded": true|false }`.
- `POST /api/v1/settings/tracker/afk-threshold` with `{ "seconds": integer }`.
- `POST /api/v1/settings/tracker/pause` with `{ "paused": true|false }`.
- `POST /api/v1/settings/runtime/audio-participation` with `{ "enabled": true|false }`.
- `POST /api/v1/settings/runtime/browser-activity` with the complete `enabled`, `port`, `token`, and `url_privacy` configuration.
- `POST /api/v1/settings/local-api/port` with `{ "port": 1024..65535 }`; continue at the returned `configuration.base_url`.
- `POST /api/v1/settings/local-api/token/rotate` only after explicit confirmation; reread `PATINA_API_TOKEN_FILE` immediately afterward.
- `POST /api/v1/system/service/restart` with `{ "confirmed": true }` only after explicit confirmation and only when capabilities include `service-lifecycle`.
- `POST /api/v1/tools/reminders` and `/api/v1/tools/software-reminder-rules` for explicit reminder creation.
- `POST /api/v1/tools/timer/*` for explicit stopwatch/countdown control.
- `POST /api/v1/tools/pomodoro/*` for explicit pomodoro control.

Browser activity configuration is a complete replacement operation. Confirm the requested port, whether synchronization should be enabled, and the URL privacy mode before sending it. Never expose either API or browser extension Tokens in analysis output.

Local API Token rotation revokes the old bearer value and existing SSE sessions, but does not change the browser extension Token. The HTTP response never contains the replacement value. A local API port conflict preserves the old listener and stored port.

Service restart returns HTTP `202` and a persistent `pending` ticket. Reconnect, read `/api/v1/system/service`, and verify the same `request_id` is `completed` by a different daemon instance before reporting success. A manual preview daemon returns `409` because it has no supervisor.

Tools writes are available only when capabilities advertise the `tools` write scope. Read `/api/v1/tools/snapshot` before state-dependent transitions and use the live OpenAPI request schemas for exact fields and bounds.

Send `Content-Type: application/json` and the same Authorization header. Do not call routes listed as planned in the human API index.
