import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";

const skillRoot = "skills/analyzing-patina-activity";
assert.equal(
  existsSync(`${skillRoot}/SKILL.md`),
  true,
  "Patina Agent Skill has not been created",
);
const skill = await readFile(`${skillRoot}/SKILL.md`, "utf8");
const mcp = await readFile(`${skillRoot}/references/mcp.md`, "utf8");
const http = await readFile(`${skillRoot}/references/http.md`, "utf8");
const analysis = await readFile(`${skillRoot}/references/analysis.md`, "utf8");
const metadata = await readFile(`${skillRoot}/agents/openai.yaml`, "utf8");
const packageJson = JSON.parse(await readFile("package.json", "utf8"));

assert.match(skill, /^name: analyzing-patina-activity$/m);
assert.match(skill, /Use when/i);
assert.match(skill, /MCP/i);
assert.match(skill, /HTTP/i);
assert.match(skill, /references\/mcp\.md/);
assert.match(skill, /references\/http\.md/);
assert.match(skill, /references\/analysis\.md/);

assert.match(mcp, /get_diagnostics/);
assert.match(mcp, /get_activity_context/);
assert.match(mcp, /query_sessions/);
assert.match(mcp, /get_active_session/);
assert.match(mcp, /query_web_activity/);

assert.match(http, /Authorization: Bearer/);
assert.match(http, /api\/v1\/openapi\.json/);
assert.match(http, /PATINA_API_TOKEN_FILE/);
assert.doesNotMatch(http, /patina_api_[a-f0-9]{8,}/);

assert.match(analysis, /diagnostics/i);
assert.match(analysis, /closed sessions/i);
assert.match(analysis, /active session/i);
assert.match(analysis, /local day/i);
assert.match(analysis, /explicit user intent/i);
assert.match(analysis, /URL/i);
assert.match(analysis, /do not infer/i);

assert.match(metadata, /display_name: "Analyze Patina Activity"/);
assert.match(metadata, /\$analyzing-patina-activity/);
assert.match(packageJson.scripts["check:frontend"], /npm run test:mcp/);
assert.match(packageJson.scripts["check:frontend"], /npm run test:agent-skill/);

console.log("Validated Patina Agent Skill transport and safety contract");
