# Patina MCP Wrapper

> Status: active reference for the local MCP wrapper.
> Scope: stdio MCP bridge from external agents to the Patina local HTTP API.

---

## 1. Purpose

The MCP wrapper lets external agents query Patina without embedding Patina-specific HTTP calls directly.

It is intentionally thin:

- It does not store data.
- It does not run analysis itself.
- It maps MCP tool calls to the local API.
- It returns Patina API responses as JSON text content.

The local HTTP API remains the source of truth for auth, schemas, and behavior. See [`api-index.md`](./api-index.md).

---

## 2. Run

From the repository root:

```bash
npm run mcp:patina
```

The npm script runs:

```bash
node --experimental-strip-types scripts/patina-mcp.ts
```

The Patina local API must be listening. During migration this can be the desktop runtime or an explicit tracking-owner `patinad`; use `/api/v1/capabilities` to verify the runtime host and write scopes.

For an MCP client, launch the Node script directly with an absolute path. Do not put `npm run` between the client and server because package-manager output can contaminate the stdio protocol stream.

---

## 3. Environment

The wrapper reads:

| Variable | Required | Default | Purpose |
|---|---:|---|---|
| `PATINA_API_BASE` | No | `http://127.0.0.1:14840` | Local API base URL |
| `PATINA_API_TOKEN` | No | none | Bearer token value |
| `PATINA_API_TOKEN_FILE` | No | `${XDG_DATA_HOME:-~/.local/share}/Patina/api_token` | Token file path |

Token resolution order:

1. `PATINA_API_TOKEN`
2. `PATINA_API_TOKEN_FILE`
3. Default Patina token file

Example:

```bash
export PATINA_API_BASE="http://127.0.0.1:14840"
export PATINA_API_TOKEN="$(cat "${XDG_DATA_HOME:-$HOME/.local/share}/Patina/api_token")"
npm run mcp:patina
```

---

## 4. Client Configuration

For an MCP client that accepts a command and environment block, configure the wrapper as a stdio server.

Example shape:

```json
{
  "mcpServers": {
    "patina": {
      "command": "node",
      "args": [
        "--experimental-strip-types",
        "/absolute/path/to/patina/scripts/patina-mcp.ts"
      ],
      "env": {
        "PATINA_API_BASE": "http://127.0.0.1:14840",
        "PATINA_API_TOKEN_FILE": "/home/user/.local/share/Patina/api_token"
      }
    }
  }
}
```

If the client does not inherit your shell environment, set either `PATINA_API_TOKEN` or `PATINA_API_TOKEN_FILE` explicitly.

### Stdio protocol

- Messages are UTF-8, newline-delimited JSON-RPC. Each request or response occupies one line.
- `notifications/initialized` and other notifications produce no response.
- The wrapper processes lines sequentially and writes only JSON-RPC messages to stdout.
- Startup and transport failures belong on stderr; stdout must not contain logs or shell/package-manager banners.
- Protocol version `2024-11-05` is currently advertised for broad client compatibility.

---

## 5. Tools

| Tool | HTTP API | Arguments | Purpose |
|---|---|---|---|
| `get_diagnostics` | `GET /api/v1/diagnostics` | none | Check Linux/window/browser/API runtime health |
| `get_runtime_settings` | `GET /api/v1/settings/runtime` | none | Read sanitized audio and browser activity settings |
| `get_current_activity` | `GET /api/v1/current` | none | Read current foreground activity snapshot |
| `query_sessions` | `GET /api/v1/sessions` | `from`, `to`, `app`, `limit` | Query closed activity sessions |
| `get_active_session` | `GET /api/v1/sessions/active` | none | Read current active session |
| `get_today_summary` | `GET /api/v1/summary/today` | none | Read local-day summary |
| `get_week_summary` | `GET /api/v1/summary/week` | none | Read local-week summary |
| `get_activity_trend` | `GET /api/v1/trend` | `period`, `granularity` | Read daily week/month trend |
| `query_web_activity` | `GET /api/v1/web-activity` | `from`, `to`, `domain`, `limit` | Query browser activity segments |
| `get_activity_context` | `GET /api/v1/ai/activity-context` | none | Fetch an AI-oriented activity context bundle |
| `get_tools_snapshot` | `GET /api/v1/tools/snapshot` | none | Fetch Tools runtime state |
| `create_reminder` | `POST /api/v1/tools/reminders` | required: `label`, `scheduledAt` | Create a reminder |
| `cancel_reminder` | `POST /api/v1/tools/reminders/{id}/cancel` | required: `id` | Cancel a reminder |
| `create_software_reminder_rule` | `POST /api/v1/tools/software-reminder-rules` | required: `appName`, `limitMs`, `message`; optional: `exeName` | Create app usage reminder |
| `disable_software_reminder_rule` | `POST /api/v1/tools/software-reminder-rules/{id}/disable` | required: `id` | Disable app usage reminder |
| `start_timer` | `POST /api/v1/tools/timer/start` | required: `mode`; optional: `durationMs`, `label` | Start stopwatch/countdown |
| `pause_timer` | `POST /api/v1/tools/timer/pause` | none | Pause timer |
| `resume_timer` | `POST /api/v1/tools/timer/resume` | none | Resume timer |
| `reset_timer` | `POST /api/v1/tools/timer/reset` | none | Reset timer |
| `add_timer_lap` | `POST /api/v1/tools/timer/laps` | none | Add stopwatch lap |
| `start_pomodoro` | `POST /api/v1/tools/pomodoro/start` | required: `focusMs`, `shortBreakMs`, `longBreakMs`, `longBreakEvery` | Start pomodoro |
| `pause_pomodoro` | `POST /api/v1/tools/pomodoro/pause` | none | Pause pomodoro |
| `resume_pomodoro` | `POST /api/v1/tools/pomodoro/resume` | none | Resume pomodoro |
| `skip_pomodoro_phase` | `POST /api/v1/tools/pomodoro/skip` | none | Skip current phase |
| `reset_pomodoro` | `POST /api/v1/tools/pomodoro/reset` | none | Reset pomodoro |
| `list_apps` | `GET /api/v1/apps` | none | List known apps |
| `set_idle_threshold` | `POST /api/v1/settings/tracker/afk-threshold` | required: `seconds` | Set idle threshold |
| `set_tracking_paused` | `POST /api/v1/settings/tracker/pause` | required: `paused` | Set tracking pause state |
| `set_audio_participation` | `POST /api/v1/settings/runtime/audio-participation` | required: `enabled` | Apply the Linux audio participation switch |
| `configure_browser_activity` | `POST /api/v1/settings/runtime/browser-activity` | required: `enabled`, `port`, `token`, `urlPrivacy` | Replace the browser activity runtime configuration |
| `classify_app` | `POST /api/v1/apps/{exe_name}/classify` | required: `exeName`, `category` | Save app category |
| `rename_app` | `POST /api/v1/apps/{exe_name}/rename` | required: `exeName`, `displayName` | Save app display name |
| `set_app_excluded` | `POST /api/v1/apps/{exe_name}/exclude` | required: `exeName`, `excluded` | Save app exclusion flag |

Argument timestamps are milliseconds since Unix epoch.

All Tools write tools require the tracking-owner daemon and the `tools` write scope. They return the complete Tools snapshot. Use them only after explicit user intent; creating reminders and starting timers are state-changing even though they are local-only.

### Errors

- Invalid tool names or missing required arguments return JSON-RPC `-32602` errors.
- Unsupported protocol methods return JSON-RPC `-32601` errors.
- Malformed JSON lines return JSON-RPC `-32700` errors.
- Patina API connection, authentication, and handler failures are tool execution errors: `tools/call` returns normal MCP content with `isError: true` and the failure text.
- A successful tool call returns the Patina HTTP response envelope as formatted JSON text.

---

## 6. Agent Skill

The repository includes [`skills/analyzing-patina-activity`](../skills/analyzing-patina-activity/SKILL.md). It has separate MCP and direct HTTP workflows and shared rules for diagnostics, active sessions, privacy, and write confirmation.

---

## 7. Current Gaps

- The wrapper does not generate tools from `/api/v1/openapi.json` yet.
- Local API configuration write-side tools are not implemented yet.
- Browser extension installation and GNOME extension installation remain app/docs workflows, not MCP tools.

`configure_browser_activity` is a complete replacement operation and must only be used after explicit confirmation. Do not echo its Token in summaries, logs, or analysis output.
