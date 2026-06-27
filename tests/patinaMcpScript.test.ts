import assert from "node:assert/strict";
import {
  handleMcpLine,
  handleMcpRequest,
  PATINA_MCP_TOOLS,
} from "../scripts/patina-mcp.ts";

let passed = 0;

async function runTest(name: string, fn: () => void | Promise<void>) {
  try {
    await fn();
    passed += 1;
    console.log(`PASS ${name}`);
  } catch (error) {
    console.error(`FAIL ${name}`);
    console.error(error);
    process.exitCode = 1;
  }
}

await runTest("Patina MCP tool list exposes core local API tools", () => {
  const toolNames = PATINA_MCP_TOOLS.map((tool) => tool.name);

  assert.deepEqual(toolNames, [
    "get_diagnostics",
    "get_current_activity",
    "get_active_session",
    "get_today_summary",
    "get_week_summary",
    "query_sessions",
    "get_activity_trend",
    "query_web_activity",
    "get_activity_context",
    "get_tools_snapshot",
    "list_apps",
    "classify_app",
    "rename_app",
    "set_app_excluded",
  ]);
});

await runTest("Patina MCP tools/list returns tool metadata", async () => {
  const response = await handleMcpRequest({
    jsonrpc: "2.0",
    id: 1,
    method: "tools/list",
  }, {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async () => ({ data: null }),
  });

  assert.equal(response.id, 1);
  assert.equal(response.result.tools.length, 14);
  assert.equal(response.result.tools[0].name, "get_diagnostics");
});

await runTest("Patina MCP write tools declare required arguments", () => {
  const requiredByTool = new Map(
    PATINA_MCP_TOOLS.map((tool) => [tool.name, tool.inputSchema.required]),
  );

  assert.deepEqual(requiredByTool.get("classify_app"), ["exeName", "category"]);
  assert.deepEqual(requiredByTool.get("rename_app"), ["exeName", "displayName"]);
  assert.deepEqual(requiredByTool.get("set_app_excluded"), ["exeName", "excluded"]);
});

await runTest("Patina MCP ignores initialized notifications", async () => {
  const response = await handleMcpRequest({
    jsonrpc: "2.0",
    method: "notifications/initialized",
  }, {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async () => ({ data: null }),
  });

  assert.equal(response, null);
});

await runTest("Patina MCP tools/call maps web activity args to query string", async () => {
  const requestedPaths: string[] = [];
  const response = await handleMcpRequest({
    jsonrpc: "2.0",
    id: 2,
    method: "tools/call",
    params: {
      name: "query_web_activity",
      arguments: {
        from: 1000,
        to: 2000,
        domain: "github.com",
        limit: 25,
      },
    },
  }, {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async (path) => {
      requestedPaths.push(path);
      return { data: { items: [] } };
    },
  });

  assert.deepEqual(requestedPaths, [
    "/api/v1/web-activity?from=1000&to=2000&domain=github.com&limit=25",
  ]);
  assert.equal(response.result.content[0].type, "text");
  assert.match(response.result.content[0].text, /"items": \[\]/);
});

await runTest("Patina MCP tools/call maps session query args to query string", async () => {
  const requestedPaths: string[] = [];
  await handleMcpRequest({
    jsonrpc: "2.0",
    id: 3,
    method: "tools/call",
    params: {
      name: "query_sessions",
      arguments: {
        from: 1000,
        to: 2000,
        app: "ghostty",
        limit: 10,
      },
    },
  }, {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async (path) => {
      requestedPaths.push(path);
      return { data: { sessions: [] } };
    },
  });

  assert.deepEqual(requestedPaths, [
    "/api/v1/sessions?from=1000&to=2000&app=ghostty&limit=10",
  ]);
});

await runTest("Patina MCP tools/call maps trend query args to query string", async () => {
  const requestedPaths: string[] = [];
  await handleMcpRequest({
    jsonrpc: "2.0",
    id: 4,
    method: "tools/call",
    params: {
      name: "get_activity_trend",
      arguments: {
        period: "month",
        granularity: "day",
      },
    },
  }, {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async (path) => {
      requestedPaths.push(path);
      return { data: { data_points: [] } };
    },
  });

  assert.deepEqual(requestedPaths, [
    "/api/v1/trend?period=month&granularity=day",
  ]);
});

await runTest("Patina MCP classify_app sends POST body", async () => {
  const calls: Array<{ path: string; init?: { method?: string; body?: unknown } }> = [];
  await handleMcpRequest({
    jsonrpc: "2.0",
    id: 5,
    method: "tools/call",
    params: {
      name: "classify_app",
      arguments: {
        exeName: "ghostty",
        category: "Development",
      },
    },
  }, {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async (path, _deps, init) => {
      calls.push({ path, init });
      return { data: { ok: true } };
    },
  });

  assert.deepEqual(calls, [
    {
      path: "/api/v1/apps/ghostty/classify",
      init: {
        method: "POST",
        body: { category: "Development" },
      },
    },
  ]);
});

await runTest("Patina MCP rename_app sends POST body", async () => {
  const calls: Array<{ path: string; init?: { method?: string; body?: unknown } }> = [];
  await handleMcpRequest({
    jsonrpc: "2.0",
    id: 6,
    method: "tools/call",
    params: {
      name: "rename_app",
      arguments: {
        exeName: "zen",
        displayName: "Zen Browser",
      },
    },
  }, {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async (path, _deps, init) => {
      calls.push({ path, init });
      return { data: { ok: true } };
    },
  });

  assert.deepEqual(calls, [
    {
      path: "/api/v1/apps/zen/rename",
      init: {
        method: "POST",
        body: { display_name: "Zen Browser" },
      },
    },
  ]);
});

await runTest("Patina MCP set_app_excluded sends POST body", async () => {
  const calls: Array<{ path: string; init?: { method?: string; body?: unknown } }> = [];
  await handleMcpRequest({
    jsonrpc: "2.0",
    id: 7,
    method: "tools/call",
    params: {
      name: "set_app_excluded",
      arguments: {
        exeName: "steam_app_default",
        excluded: true,
      },
    },
  }, {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async (path, _deps, init) => {
      calls.push({ path, init });
      return { data: { ok: true } };
    },
  });

  assert.deepEqual(calls, [
    {
      path: "/api/v1/apps/steam_app_default/exclude",
      init: {
        method: "POST",
        body: { excluded: true },
      },
    },
  ]);
});

await runTest("Patina MCP reports API failures as tool results", async () => {
  const response = await handleMcpRequest({
    jsonrpc: "2.0",
    id: 8,
    method: "tools/call",
    params: {
      name: "get_diagnostics",
      arguments: {},
    },
  }, {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async () => {
      throw new Error("connection refused");
    },
  });

  assert.equal(response.error, undefined);
  assert.equal(response.result.isError, true);
  assert.match(response.result.content[0].text, /connection refused/);
});

await runTest("Patina MCP stdio uses newline-delimited JSON and processes each message once", async () => {
  const deps = {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async () => ({ data: null }),
  };
  const lines = await Promise.all([
    {
      jsonrpc: "2.0",
      id: 10,
      method: "initialize",
      params: {
        protocolVersion: "2024-11-05",
        capabilities: {},
        clientInfo: { name: "test-client", version: "1.0.0" },
      },
    },
    {
      jsonrpc: "2.0",
      method: "notifications/initialized",
    },
    {
      jsonrpc: "2.0",
      id: 11,
      method: "tools/list",
    },
  ].map((message) => handleMcpLine(JSON.stringify(message), deps)));
  const encodedResponses = lines.filter((line): line is string => line !== null);
  const responses = encodedResponses.map((line) => JSON.parse(line));

  assert.deepEqual(responses.map((response) => response.id), [10, 11]);
  assert.equal(responses[1].result.tools.length, 14);
  assert.equal(encodedResponses.every((line) => !line.startsWith("Content-Length:")), true);
});

console.log(`Passed ${passed} Patina MCP script tests`);
