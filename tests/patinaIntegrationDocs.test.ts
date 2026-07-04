import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const apiDocs = await readFile("docs/api-index.md", "utf8");
const mcpDocs = await readFile("docs/mcp-wrapper.md", "utf8");

const implementedEndpoints = [
  "GET /api/v1/health",
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
  "POST /api/v1/apps/{exe_name}/classify",
  "POST /api/v1/apps/{exe_name}/rename",
  "POST /api/v1/apps/{exe_name}/exclude",
  "GET /api/v1/settings/tracker",
  "POST /api/v1/settings/tracker/afk-threshold",
  "GET /api/v1/tools/snapshot",
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

console.log(`Validated ${implementedEndpoints.length} API sections and MCP transport documentation`);

function escapeRegExp(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
