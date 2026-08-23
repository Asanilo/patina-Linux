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
    "get_runtime_settings",
    "get_local_api_configuration",
    "set_local_api_port",
    "rotate_local_api_token",
    "get_current_activity",
    "get_active_session",
    "get_today_summary",
    "get_week_summary",
    "query_sessions",
    "get_activity_trend",
    "query_web_activity",
    "get_activity_context",
    "get_tools_snapshot",
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
    "list_apps",
    "set_idle_threshold",
    "set_tracking_paused",
    "set_audio_participation",
    "configure_browser_activity",
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
  assert.equal(response.result.tools.length, 36);
  assert.equal(response.result.tools[0].name, "get_diagnostics");
});

await runTest("Patina MCP write tools declare required arguments", () => {
  const requiredByTool = new Map(
    PATINA_MCP_TOOLS.map((tool) => [tool.name, tool.inputSchema.required]),
  );

  assert.deepEqual(requiredByTool.get("classify_app"), ["exeName", "category"]);
  assert.deepEqual(requiredByTool.get("rename_app"), ["exeName", "displayName"]);
  assert.deepEqual(requiredByTool.get("set_app_excluded"), ["exeName", "excluded"]);
  assert.deepEqual(requiredByTool.get("set_idle_threshold"), ["seconds"]);
  assert.deepEqual(requiredByTool.get("set_tracking_paused"), ["paused"]);
  assert.deepEqual(requiredByTool.get("set_audio_participation"), ["enabled"]);
  assert.deepEqual(requiredByTool.get("set_local_api_port"), ["port"]);
  assert.deepEqual(requiredByTool.get("rotate_local_api_token"), ["confirmed"]);
  assert.deepEqual(
    requiredByTool.get("configure_browser_activity"),
    ["enabled", "port", "token", "urlPrivacy"],
  );
  assert.deepEqual(requiredByTool.get("create_reminder"), ["label", "scheduledAt"]);
  assert.deepEqual(requiredByTool.get("cancel_reminder"), ["id"]);
  assert.deepEqual(
    requiredByTool.get("create_software_reminder_rule"),
    ["appName", "limitMs", "message"],
  );
  assert.deepEqual(requiredByTool.get("start_timer"), ["mode"]);
  assert.deepEqual(
    requiredByTool.get("start_pomodoro"),
    ["focusMs", "shortBreakMs", "longBreakMs", "longBreakEvery"],
  );
});

await runTest("Patina MCP Tools writes map to bounded daemon API calls", async () => {
  const calls: Array<{ path: string; init?: Record<string, unknown> }> = [];
  const deps = {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async (path: string, _auth: unknown, init?: Record<string, unknown>) => {
      calls.push({ path, init });
      return { data: { sampled_at_ms: 1 } };
    },
  };

  await handleMcpRequest({
    id: 90,
    method: "tools/call",
    params: {
      name: "create_reminder",
      arguments: { label: "Review", scheduledAt: 1_900_000_000_000 },
    },
  }, deps);
  await handleMcpRequest({
    id: 91,
    method: "tools/call",
    params: {
      name: "start_pomodoro",
      arguments: {
        focusMs: 1_500_000,
        shortBreakMs: 300_000,
        longBreakMs: 900_000,
        longBreakEvery: 4,
      },
    },
  }, deps);
  await handleMcpRequest({
    id: 92,
    method: "tools/call",
    params: { name: "pause_timer", arguments: {} },
  }, deps);

  assert.deepEqual(calls, [
    {
      path: "/api/v1/tools/reminders",
      init: {
        method: "POST",
        body: { label: "Review", scheduled_at: 1_900_000_000_000 },
      },
    },
    {
      path: "/api/v1/tools/pomodoro/start",
      init: {
        method: "POST",
        body: {
          focus_ms: 1_500_000,
          short_break_ms: 300_000,
          long_break_ms: 900_000,
          long_break_every: 4,
        },
      },
    },
    { path: "/api/v1/tools/timer/pause", init: { method: "POST" } },
  ]);
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

await runTest("Patina MCP reads sanitized runtime settings", async () => {
  const requestedPaths: string[] = [];
  const response = await handleMcpRequest({
    jsonrpc: "2.0",
    id: 25,
    method: "tools/call",
    params: {
      name: "get_runtime_settings",
      arguments: {},
    },
  }, {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async (path) => {
      requestedPaths.push(path);
      return {
        data: {
          audio_participation_enabled: true,
          browser_activity: {
            enabled: true,
            port: 12345,
            token_present: true,
            url_privacy: "domain_only",
          },
        },
      };
    },
  });

  assert.deepEqual(requestedPaths, ["/api/v1/settings/runtime"]);
  assert.match(response.result.content[0].text, /"token_present": true/);
  assert.doesNotMatch(response.result.content[0].text, /browser-token/);
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

await runTest("Patina MCP tracker setting tools send bounded POST bodies", async () => {
  const calls: Array<{ path: string; init?: { method?: string; body?: unknown } }> = [];
  const deps = {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async (path: string, _deps: unknown, init?: { method?: string; body?: unknown }) => {
      calls.push({ path, init });
      return { data: { ok: true } };
    },
  };

  await handleMcpRequest({
    jsonrpc: "2.0",
    id: 20,
    method: "tools/call",
    params: {
      name: "set_idle_threshold",
      arguments: { seconds: 900 },
    },
  }, deps);
  await handleMcpRequest({
    jsonrpc: "2.0",
    id: 21,
    method: "tools/call",
    params: {
      name: "set_tracking_paused",
      arguments: { paused: true },
    },
  }, deps);

  assert.deepEqual(calls, [
    {
      path: "/api/v1/settings/tracker/afk-threshold",
      init: { method: "POST", body: { seconds: 900 } },
    },
    {
      path: "/api/v1/settings/tracker/pause",
      init: { method: "POST", body: { paused: true } },
    },
  ]);
});

await runTest("Patina MCP rejects out-of-range idle thresholds before the API call", async () => {
  let called = false;
  const response = await handleMcpRequest({
    jsonrpc: "2.0",
    id: 22,
    method: "tools/call",
    params: {
      name: "set_idle_threshold",
      arguments: { seconds: 30 },
    },
  }, {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async () => {
      called = true;
      return { data: { ok: true } };
    },
  });

  assert.equal(called, false);
  assert.equal(response.error.code, -32602);
  assert.match(response.error.message, /60 through 86400/);
});

await runTest("Patina MCP runtime setting tools send complete replacement bodies", async () => {
  const calls: Array<{ path: string; init?: { method?: string; body?: unknown } }> = [];
  const deps = {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "token",
    callApi: async (path: string, _deps: unknown, init?: { method?: string; body?: unknown }) => {
      calls.push({ path, init });
      return { data: { ok: true } };
    },
  };

  await handleMcpRequest({
    jsonrpc: "2.0",
    id: 23,
    method: "tools/call",
    params: {
      name: "set_audio_participation",
      arguments: { enabled: false },
    },
  }, deps);
  await handleMcpRequest({
    jsonrpc: "2.0",
    id: 24,
    method: "tools/call",
    params: {
      name: "configure_browser_activity",
      arguments: {
        enabled: true,
        port: 12345,
        token: " browser-token ",
        urlPrivacy: "domain_only",
      },
    },
  }, deps);

  assert.deepEqual(calls, [
    {
      path: "/api/v1/settings/runtime/audio-participation",
      init: { method: "POST", body: { enabled: false } },
    },
    {
      path: "/api/v1/settings/runtime/browser-activity",
      init: {
        method: "POST",
        body: {
          enabled: true,
          port: 12345,
          token: "browser-token",
          url_privacy: "domain_only",
        },
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

await runTest("Patina MCP local API controls update their own connection safely", async () => {
  const calls: Array<{ path: string; apiBase: string; apiToken: string }> = [];
  const deps = {
    apiBase: "http://127.0.0.1:14840",
    apiToken: "old-token",
    refreshApiToken: async () => "new-token",
    callApi: async (path, requestDeps) => {
      calls.push({ path, apiBase: requestDeps.apiBase, apiToken: requestDeps.apiToken });
      if (path.endsWith("/port")) {
        return {
          data: {
            configuration: { base_url: "http://127.0.0.1:15555" },
          },
        };
      }
      return { data: { reauthentication_required: true } };
    },
  };

  await handleMcpRequest({
    jsonrpc: "2.0",
    id: 20,
    method: "tools/call",
    params: { name: "set_local_api_port", arguments: { port: 15555 } },
  }, deps);
  assert.equal(deps.apiBase, "http://127.0.0.1:15555");

  const rejected = await handleMcpRequest({
    jsonrpc: "2.0",
    id: 21,
    method: "tools/call",
    params: { name: "rotate_local_api_token", arguments: { confirmed: false } },
  }, deps);
  assert.match(rejected.error.message, /confirmed=true/);

  await handleMcpRequest({
    jsonrpc: "2.0",
    id: 22,
    method: "tools/call",
    params: { name: "rotate_local_api_token", arguments: { confirmed: true } },
  }, deps);
  assert.equal(deps.apiToken, "new-token");
  assert.deepEqual(calls, [
    {
      path: "/api/v1/settings/local-api/port",
      apiBase: "http://127.0.0.1:14840",
      apiToken: "old-token",
    },
    {
      path: "/api/v1/settings/local-api/token/rotate",
      apiBase: "http://127.0.0.1:15555",
      apiToken: "old-token",
    },
  ]);
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
  assert.equal(responses[1].result.tools.length, 36);
  assert.equal(encodedResponses.every((line) => !line.startsWith("Content-Length:")), true);
});

console.log(`Passed ${passed} Patina MCP script tests`);
