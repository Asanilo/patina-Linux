# MCP Workflow

Use this workflow when the host exposes the Patina MCP server. Tool names may carry a host-specific server prefix; match the final tool name below.

## Read Tools

| Tool | Arguments | Use |
|---|---|---|
| `get_diagnostics` | none | Validate window tracking, tracker runtime, and browser bridge |
| `get_runtime_settings` | none | Read sanitized audio and browser activity configuration |
| `get_current_activity` | none | Read the foreground-window sample |
| `get_active_session` | none | Read the realtime active session or `null` |
| `get_today_summary` | none | Read the local-day aggregate |
| `get_week_summary` | none | Read the Monday-based local-week aggregate |
| `query_sessions` | optional `from`, `to`, `app`, `limit` | Query closed sessions by start time |
| `get_activity_trend` | optional `period`, `granularity` | Read `week` or `month`; granularity is currently `day` |
| `query_web_activity` | optional `from`, `to`, `domain`, `limit` | Query browser segments |
| `get_activity_context` | none | Fetch diagnostics, active session, today/week summaries, and 25 recent web segments |
| `get_tools_snapshot` | none | Read reminders, timer, and pomodoro state |
| `list_apps` | none | Read known app mappings |

All timestamp arguments are Unix epoch milliseconds.

## Write Tools

Use only after explicit user intent:

| Tool | Required arguments | Effect |
|---|---|---|
| `classify_app` | `exeName`, `category` | Persist an app category |
| `rename_app` | `exeName`, `displayName` | Persist an app display name |
| `set_app_excluded` | `exeName`, `excluded` | Change whether the app contributes to statistics |
| `set_idle_threshold` | `seconds` | Set the idle threshold; accepted range is 60 through 86400 seconds |
| `set_tracking_paused` | `paused` | Explicitly pause or resume automatic tracking |
| `set_audio_participation` | `enabled` | Enable or disable the Linux audio participation signal |
| `configure_browser_activity` | `enabled`, `port`, `token`, `urlPrivacy` | Replace the complete browser extension bridge configuration |
| `create_reminder` | `label`, `scheduledAt` | Create a future reminder |
| `cancel_reminder` | `id` | Cancel a scheduled reminder |
| `create_software_reminder_rule` | `appName`, `limitMs`, `message`; optional `exeName` | Create a daily app usage reminder |
| `disable_software_reminder_rule` | `id` | Disable an app usage reminder |
| `start_timer` | `mode`; countdown also needs `durationMs`; optional `label` | Start stopwatch/countdown |
| `pause_timer`, `resume_timer`, `reset_timer`, `add_timer_lap` | none | Control current timer |
| `start_pomodoro` | `focusMs`, `shortBreakMs`, `longBreakMs`, `longBreakEvery` | Start pomodoro |
| `pause_pomodoro`, `resume_pomodoro`, `skip_pomodoro_phase`, `reset_pomodoro` | none | Control current pomodoro |

Before app writes, use `list_apps` to verify the exact `exe_name`. Before browser configuration, restate the port and URL privacy mode without printing the Token. Before Tools writes, read `get_tools_snapshot` when the existing state affects the requested transition. Do not infer permission for any write from a general request to analyze data.

## Recommended Sequences

### Overview

1. Call `get_diagnostics`.
2. Call `get_activity_context`.
3. Inspect any component-level `error` before interpreting the bundle.

### Custom session investigation

1. Convert the requested local range to epoch milliseconds.
2. Call `query_sessions` for closed sessions.
3. Call `get_active_session` when the range reaches the present.
4. Do not claim clipped range totals from `query_sessions`; use the HTTP summary-range endpoint when exact clipping is required.

### Browser investigation

1. Use `get_runtime_settings` to read configured port/privacy state without exposing the Token.
2. Confirm `web_activity_bridge.enabled` and recent reporting in diagnostics.
3. Call `query_web_activity` with the narrowest useful range/domain.
4. Respect `url: null`; it represents Patina's configured privacy mode, not missing data to reconstruct.

## Errors

- JSON-RPC errors indicate invalid method/tool arguments.
- A tool result with `isError: true` indicates API connection, authentication, or handler failure.
- Report the actionable failure without exposing tokens or unrelated private payloads.
