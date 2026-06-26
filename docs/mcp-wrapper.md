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

The Patina desktop app must be running, and the local API must be listening.

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
      "command": "npm",
      "args": ["run", "mcp:patina"],
      "cwd": "/home/arinp22/code/patina",
      "env": {
        "PATINA_API_BASE": "http://127.0.0.1:14840"
      }
    }
  }
}
```

If the client does not inherit your shell environment, set either `PATINA_API_TOKEN` or `PATINA_API_TOKEN_FILE` explicitly.

---

## 5. Tools

| Tool | HTTP API | Arguments | Purpose |
|---|---|---|---|
| `get_diagnostics` | `GET /api/v1/diagnostics` | none | Check Linux/window/browser/API runtime health |
| `get_current_activity` | `GET /api/v1/current` | none | Read current foreground activity snapshot |
| `query_sessions` | `GET /api/v1/sessions` | `from`, `to`, `app`, `limit` | Query closed activity sessions |
| `get_active_session` | `GET /api/v1/sessions/active` | none | Read current active session |
| `get_today_summary` | `GET /api/v1/summary/today` | none | Read local-day summary |
| `get_week_summary` | `GET /api/v1/summary/week` | none | Read local-week summary |
| `get_activity_trend` | `GET /api/v1/trend` | `period`, `granularity` | Read daily week/month trend |
| `query_web_activity` | `GET /api/v1/web-activity` | `from`, `to`, `domain`, `limit` | Query browser activity segments |
| `get_activity_context` | `GET /api/v1/ai/activity-context` | none | Fetch an AI-oriented activity context bundle |
| `get_tools_snapshot` | `GET /api/v1/tools/snapshot` | none | Fetch Tools runtime state |
| `list_apps` | `GET /api/v1/apps` | none | List known apps |
| `classify_app` | `POST /api/v1/apps/{exe_name}/classify` | `exeName`, `category` | Save app category |
| `rename_app` | `POST /api/v1/apps/{exe_name}/rename` | `exeName`, `displayName` | Save app display name |
| `set_app_excluded` | `POST /api/v1/apps/{exe_name}/exclude` | `exeName`, `excluded` | Save app exclusion flag |

Argument timestamps are milliseconds since Unix epoch.

---

## 6. Current Gaps

- The wrapper does not generate tools from `/api/v1/openapi.json` yet.
- Tracker settings write-side tools are not implemented yet.
- Local API configuration write-side tools are not implemented yet.
- Tools write-side actions such as creating reminders, starting timers, and pausing timers are not implemented yet.
- Browser extension installation and GNOME extension installation remain app/docs workflows, not MCP tools.

