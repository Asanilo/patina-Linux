import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const apiDocs = await readFile("docs/api-index.md", "utf8");
const mcpDocs = await readFile("docs/mcp-wrapper.md", "utf8");

const implementedEndpoints = [
  "GET /api/v1/health",
  "GET /api/v1/capabilities",
  "GET /api/v1/events",
  "GET /api/v1/openapi.json",
  "GET /api/v1/diagnostics",
  "GET /api/v1/current",
  "GET /api/v1/sessions",
  "GET /api/v1/sessions/active",
  "GET /api/v1/summary/today",
  "GET /api/v1/summary/range",
  "GET /api/v1/summary/week",
  "GET /api/v1/trend",
  "GET /api/v1/web-activity",
  "GET /api/v1/ai/activity-context",
  "GET /api/v1/apps",
  "GET /api/v1/imports",
  "POST /api/v1/imports/canonical/commit",
  "POST /api/v1/imports/{batch_id}/delete",
  "GET /api/v1/backups/schedule",
  "POST /api/v1/backups/schedule",
  "POST /api/v1/apps/{exe_name}/classify",
  "POST /api/v1/apps/{exe_name}/rename",
  "POST /api/v1/apps/{exe_name}/exclude",
  "GET /api/v1/settings/tracker",
  "POST /api/v1/settings/tracker/afk-threshold",
  "POST /api/v1/settings/tracker/pause",
  "POST /api/v1/settings/classification",
  "POST /api/v1/settings/app",
  "GET /api/v1/settings/runtime",
  "POST /api/v1/settings/runtime/audio-participation",
  "POST /api/v1/settings/runtime/browser-activity",
  "GET /api/v1/settings/local-api",
  "POST /api/v1/settings/local-api/port",
  "POST /api/v1/settings/local-api/token/rotate",
  "POST /api/v1/data/cleanup",
  "POST /api/v1/data/window-titles/clear",
  "POST /api/v1/data/apps/delete",
  "GET /api/v1/system/service",
  "POST /api/v1/system/service/restart",
  "GET /api/v1/tools/snapshot",
  "POST /api/v1/tools/reminders",
  "POST /api/v1/tools/reminders/{id}/cancel",
  "POST /api/v1/tools/software-reminder-rules",
  "POST /api/v1/tools/software-reminder-rules/{id}/disable",
  "POST /api/v1/tools/timer/start",
  "POST /api/v1/tools/timer/pause",
  "POST /api/v1/tools/timer/resume",
  "POST /api/v1/tools/timer/reset",
  "POST /api/v1/tools/timer/laps",
  "POST /api/v1/tools/pomodoro/start",
  "POST /api/v1/tools/pomodoro/pause",
  "POST /api/v1/tools/pomodoro/resume",
  "POST /api/v1/tools/pomodoro/skip",
  "POST /api/v1/tools/pomodoro/reset",
];

for (const endpoint of implementedEndpoints) {
  assert.match(
    apiDocs,
    new RegExp("^### `" + escapeRegExp(endpoint) + "`$", "m"),
    `docs/api-index.md is missing an endpoint section for ${endpoint}`,
  );
}

assert.match(mcpDocs, /newline-delimited JSON/i);
assert.match(mcpDocs, /"command": "node"/);
assert.match(mcpDocs, /"--experimental-strip-types"/);
assert.match(mcpDocs, /\/absolute\/path\/to\/patina\/scripts\/patina-mcp\.ts/);
assert.match(mcpDocs, /notifications\/initialized/);
assert.match(mcpDocs, /tool execution errors/i);
assert.match(apiDocs, /native > import_exact > import_bucket/);
assert.match(apiDocs, /aggregate-only hour buckets/i);
assert.match(mcpDocs, /native > import_exact > import_bucket/);

for (const tool of [
  "create_reminder",
  "cancel_reminder",
  "create_software_reminder_rule",
  "disable_software_reminder_rule",
  "start_timer",
  "pause_timer",
  "resume_timer",
  "reset_timer",
  "add_timer_lap",
  "start_pomodoro",
  "pause_pomodoro",
  "resume_pomodoro",
  "skip_pomodoro_phase",
  "reset_pomodoro",
  "get_local_api_configuration",
  "set_local_api_port",
  "rotate_local_api_token",
  "get_daemon_service",
  "restart_daemon_service",
]) {
  assert.match(mcpDocs, new RegExp("`" + tool + "`"), `MCP docs are missing ${tool}`);
}

console.log(`Validated ${implementedEndpoints.length} API sections and MCP transport documentation`);

function escapeRegExp(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
