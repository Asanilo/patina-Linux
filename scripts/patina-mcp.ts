import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import process from "node:process";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

type JsonRpcRequest = {
  jsonrpc?: string;
  id?: string | number | null;
  method?: string;
  params?: Record<string, unknown>;
};

type JsonRpcResponse = {
  jsonrpc: "2.0";
  id: string | number | null;
  result?: Record<string, unknown>;
  error?: {
    code: number;
    message: string;
  };
};

type PatinaMcpTool = {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
};

type McpDeps = {
  apiBase: string;
  apiToken: string;
  refreshApiToken?: (tokenPath?: string) => Promise<string>;
  callApi: (
    path: string,
    deps: Pick<McpDeps, "apiBase" | "apiToken">,
    init?: PatinaApiCallInit,
  ) => Promise<unknown>;
};

type PatinaApiCallInit = {
  method?: "GET" | "POST";
  body?: unknown;
};

const DEFAULT_API_BASE = "http://127.0.0.1:14840";
const MCP_PROTOCOL_VERSION = "2024-11-05";

export const PATINA_MCP_TOOLS: PatinaMcpTool[] = [
  {
    name: "get_diagnostics",
    description: "Read Patina platform, tracker runtime, and browser bridge diagnostics.",
    inputSchema: objectSchema({}),
  },
  {
    name: "get_runtime_settings",
    description: "Read sanitized Patina audio and browser activity runtime settings.",
    inputSchema: objectSchema({}),
  },
  {
    name: "get_local_api_configuration",
    description: "Read sanitized Patina local API listener and credential-file configuration.",
    inputSchema: objectSchema({}),
  },
  {
    name: "set_local_api_port",
    description: "Move the Patina local API listener to another loopback port. This changes local service configuration.",
    inputSchema: objectSchema({
      port: { type: "integer", minimum: 1024, maximum: 65535 },
    }, ["port"]),
  },
  {
    name: "rotate_local_api_token",
    description: "Rotate the Patina local API Token and revoke existing clients. Requires explicit confirmation.",
    inputSchema: objectSchema({
      confirmed: { type: "boolean", description: "Must be true after explicit user confirmation." },
    }, ["confirmed"]),
  },
  {
    name: "get_daemon_service",
    description: "Read the patinad systemd user-service identity and latest restart ticket.",
    inputSchema: objectSchema({}),
  },
  {
    name: "restart_daemon_service",
    description: "Request a graceful patinad restart through systemd. Requires explicit confirmation and returns a ticket to verify after reconnecting.",
    inputSchema: objectSchema({
      confirmed: { type: "boolean", description: "Must be true after explicit user confirmation." },
    }, ["confirmed"]),
  },
  {
    name: "get_current_activity",
    description: "Read the current foreground window snapshot.",
    inputSchema: objectSchema({}),
  },
  {
    name: "get_active_session",
    description: "Read the current active tracking session, if one exists.",
    inputSchema: objectSchema({}),
  },
  {
    name: "get_today_summary",
    description: "Read today's local-time Patina activity summary.",
    inputSchema: objectSchema({}),
  },
  {
    name: "get_week_summary",
    description: "Read this week's local-time Patina activity summary.",
    inputSchema: objectSchema({}),
  },
  {
    name: "query_sessions",
    description: "Query closed Patina sessions.",
    inputSchema: objectSchema({
      from: { type: "number", description: "Optional lower start timestamp in milliseconds." },
      to: { type: "number", description: "Optional upper start timestamp in milliseconds." },
      app: { type: "string", description: "Optional exact exe_name filter." },
      limit: { type: "number", description: "Optional result limit." },
    }),
  },
  {
    name: "get_activity_trend",
    description: "Read daily activity trend data for week or month.",
    inputSchema: objectSchema({
      period: { type: "string", description: "week or month." },
      granularity: { type: "string", description: "Currently day." },
    }),
  },
  {
    name: "query_web_activity",
    description: "Query browser activity segments captured by the Patina browser extension.",
    inputSchema: objectSchema({
      from: { type: "number", description: "Optional lower timestamp in milliseconds." },
      to: { type: "number", description: "Optional upper timestamp in milliseconds." },
      domain: { type: "string", description: "Optional normalized domain filter." },
      limit: { type: "number", description: "Optional result limit." },
    }),
  },
  {
    name: "get_activity_context",
    description: "Read aggregated Patina context for external AI analysis.",
    inputSchema: objectSchema({}),
  },
  {
    name: "get_tools_snapshot",
    description: "Read the current Patina Tools runtime snapshot.",
    inputSchema: objectSchema({}),
  },
  {
    name: "create_reminder",
    description: "Create a Patina reminder. This changes local Tools state.",
    inputSchema: objectSchema({
      label: { type: "string", maxLength: 256, description: "Reminder label." },
      scheduledAt: { type: "integer", description: "Future local timestamp in milliseconds." },
    }, ["label", "scheduledAt"]),
  },
  {
    name: "cancel_reminder",
    description: "Cancel a scheduled Patina reminder. This changes local Tools state.",
    inputSchema: positiveIdSchema("Reminder ID."),
  },
  {
    name: "create_software_reminder_rule",
    description: "Create a daily software usage reminder rule. This changes local Tools state.",
    inputSchema: objectSchema({
      appName: { type: "string", minLength: 1, maxLength: 256 },
      exeName: { type: ["string", "null"], maxLength: 256 },
      limitMs: { type: "integer", minimum: 60000, maximum: 86400000 },
      message: { type: "string", maxLength: 1024 },
    }, ["appName", "limitMs", "message"]),
  },
  {
    name: "disable_software_reminder_rule",
    description: "Disable a software usage reminder rule. This changes local Tools state.",
    inputSchema: positiveIdSchema("Software reminder rule ID."),
  },
  {
    name: "start_timer",
    description: "Start a Patina stopwatch or countdown. This changes local Tools state.",
    inputSchema: objectSchema({
      mode: { type: "string", enum: ["stopwatch", "countdown"] },
      durationMs: { type: ["integer", "null"], minimum: 60000, maximum: 10800000 },
      label: { type: ["string", "null"], maxLength: 256 },
    }, ["mode"]),
  },
  ...toolsActionMetadata(),
  {
    name: "start_pomodoro",
    description: "Start a Patina Pomodoro run. This changes local Tools state.",
    inputSchema: objectSchema({
      focusMs: { type: "integer", minimum: 60000, maximum: 10800000 },
      shortBreakMs: { type: "integer", minimum: 60000, maximum: 3600000 },
      longBreakMs: { type: "integer", minimum: 60000, maximum: 7200000 },
      longBreakEvery: { type: "integer", minimum: 2, maximum: 12 },
    }, ["focusMs", "shortBreakMs", "longBreakMs", "longBreakEvery"]),
  },
  ...pomodoroActionMetadata(),
  {
    name: "list_apps",
    description: "List known apps from recorded Patina sessions.",
    inputSchema: objectSchema({}),
  },
  {
    name: "set_idle_threshold",
    description: "Set the Patina idle threshold in seconds.",
    inputSchema: objectSchema({
      seconds: {
        type: "integer",
        minimum: 60,
        maximum: 86400,
        description: "Idle threshold in seconds.",
      },
    }, ["seconds"]),
  },
  {
    name: "set_tracking_paused",
    description: "Set whether Patina automatic tracking is paused.",
    inputSchema: objectSchema({
      paused: { type: "boolean", description: "Whether tracking should be paused." },
    }, ["paused"]),
  },
  {
    name: "set_audio_participation",
    description: "Enable or disable Patina's Linux audio participation signal.",
    inputSchema: objectSchema({
      enabled: { type: "boolean", description: "Whether audio participation is enabled." },
    }, ["enabled"]),
  },
  {
    name: "configure_browser_activity",
    description: "Replace Patina's browser activity listener, token, and URL privacy settings.",
    inputSchema: objectSchema({
      enabled: { type: "boolean", description: "Whether browser activity synchronization is enabled." },
      port: {
        type: "integer",
        minimum: 1024,
        maximum: 65535,
        description: "Loopback browser activity listener port.",
      },
      token: {
        type: "string",
        maxLength: 512,
        description: "Browser extension bearer token.",
      },
      urlPrivacy: {
        type: "string",
        enum: ["full", "strip_query", "domain_only"],
        description: "Stored URL detail level.",
      },
    }, ["enabled", "port", "token", "urlPrivacy"]),
  },
  {
    name: "classify_app",
    description: "Assign a Patina category to an app exe_name.",
    inputSchema: objectSchema({
      exeName: { type: "string", description: "Exact app exe_name to classify." },
      category: { type: "string", description: "Category name to assign." },
    }, ["exeName", "category"]),
  },
  {
    name: "rename_app",
    description: "Assign a Patina display name to an app exe_name.",
    inputSchema: objectSchema({
      exeName: { type: "string", description: "Exact app exe_name to rename." },
      displayName: { type: "string", description: "Display name to assign." },
    }, ["exeName", "displayName"]),
  },
  {
    name: "set_app_excluded",
    description: "Set whether an app is excluded from Patina activity statistics.",
    inputSchema: objectSchema({
      exeName: { type: "string", description: "Exact app exe_name to update." },
      excluded: { type: "boolean", description: "Whether the app should be excluded." },
    }, ["exeName", "excluded"]),
  },
];

export async function handleMcpRequest(
  request: JsonRpcRequest,
  deps: McpDeps,
): Promise<JsonRpcResponse | null> {
  const id = request.id ?? null;

  if (request.id === undefined) {
    return null;
  }

  if (request.method === "initialize") {
    return ok(id, {
      protocolVersion: MCP_PROTOCOL_VERSION,
      capabilities: { tools: {} },
      serverInfo: {
        name: "patina-local-api",
        version: "0.1.0",
      },
    });
  }

  if (request.method === "tools/list") {
    return ok(id, { tools: PATINA_MCP_TOOLS });
  }

  if (request.method === "tools/call") {
    const name = stringField(request.params, "name");
    const args = objectField(request.params, "arguments");
    if (!name) {
      return error(id, -32602, "tools/call requires params.name");
    }

    const apiRequest = toolNameToApiRequest(name, args);
    if (!apiRequest) {
      return error(id, -32602, `Unknown Patina MCP tool: ${name}`);
    }
    if ("error" in apiRequest) {
      return error(id, -32602, apiRequest.error);
    }

    try {
      const payload = await deps.callApi(apiRequest.path, deps, apiRequest.init);
      await applyConnectionChange(name, payload, deps);
      return ok(id, {
        content: [
          {
            type: "text",
            text: JSON.stringify(payload, null, 2),
          },
        ],
      });
    } catch (apiError) {
      return ok(id, {
        content: [
          {
            type: "text",
            text: apiError instanceof Error ? apiError.message : String(apiError),
          },
        ],
        isError: true,
      });
    }
  }

  if (request.method === "ping") {
    return ok(id, {});
  }

  return error(id, -32601, `Unsupported method: ${request.method ?? "unknown"}`);
}

export async function handleMcpLine(line: string, deps: McpDeps) {
  let message: JsonRpcRequest;
  try {
    message = JSON.parse(line) as JsonRpcRequest;
  } catch {
    return JSON.stringify(error(null, -32700, "Parse error"));
  }

  const response = await handleMcpRequest(message, deps);
  return response ? JSON.stringify(response) : null;
}

async function callPatinaApi(
  path: string,
  deps: Pick<McpDeps, "apiBase" | "apiToken">,
  init: PatinaApiCallInit = {},
) {
  const url = new URL(path, deps.apiBase);
  const response = await fetch(url, {
    method: init.method ?? "GET",
    headers: {
      Authorization: `Bearer ${deps.apiToken}`,
      ...(init.body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    body: init.body === undefined ? undefined : JSON.stringify(init.body),
  });
  const text = await response.text();
  let payload: unknown = null;
  if (text.trim()) {
    payload = JSON.parse(text);
  }
  if (!response.ok) {
    throw new Error(`Patina API ${response.status}: ${text}`);
  }
  return payload;
}

function toolNameToApiRequest(name: string, args: Record<string, unknown>) {
  switch (name) {
    case "get_diagnostics":
      return getRequest("/api/v1/diagnostics");
    case "get_runtime_settings":
      return getRequest("/api/v1/settings/runtime");
    case "get_local_api_configuration":
      return getRequest("/api/v1/settings/local-api");
    case "set_local_api_port":
      return setLocalApiPortRequest(args);
    case "rotate_local_api_token":
      return rotateLocalApiTokenRequest(args);
    case "get_daemon_service":
      return getRequest("/api/v1/system/service");
    case "restart_daemon_service":
      return restartDaemonServiceRequest(args);
    case "get_current_activity":
      return getRequest("/api/v1/current");
    case "get_active_session":
      return getRequest("/api/v1/sessions/active");
    case "get_today_summary":
      return getRequest("/api/v1/summary/today");
    case "get_week_summary":
      return getRequest("/api/v1/summary/week");
    case "query_sessions":
      return getRequest(sessionsPath(args));
    case "get_activity_trend":
      return getRequest(trendPath(args));
    case "query_web_activity":
      return getRequest(webActivityPath(args));
    case "get_activity_context":
      return getRequest("/api/v1/ai/activity-context");
    case "get_tools_snapshot":
      return getRequest("/api/v1/tools/snapshot");
    case "create_reminder":
      return createReminderRequest(args);
    case "cancel_reminder":
      return idActionRequest(args, "/api/v1/tools/reminders", "cancel_reminder", "cancel");
    case "create_software_reminder_rule":
      return createSoftwareReminderRuleRequest(args);
    case "disable_software_reminder_rule":
      return idActionRequest(
        args,
        "/api/v1/tools/software-reminder-rules",
        "disable_software_reminder_rule",
        "disable",
      );
    case "start_timer":
      return startTimerRequest(args);
    case "pause_timer":
      return postRequest("/api/v1/tools/timer/pause");
    case "resume_timer":
      return postRequest("/api/v1/tools/timer/resume");
    case "reset_timer":
      return postRequest("/api/v1/tools/timer/reset");
    case "add_timer_lap":
      return postRequest("/api/v1/tools/timer/laps");
    case "start_pomodoro":
      return startPomodoroRequest(args);
    case "pause_pomodoro":
      return postRequest("/api/v1/tools/pomodoro/pause");
    case "resume_pomodoro":
      return postRequest("/api/v1/tools/pomodoro/resume");
    case "skip_pomodoro_phase":
      return postRequest("/api/v1/tools/pomodoro/skip");
    case "reset_pomodoro":
      return postRequest("/api/v1/tools/pomodoro/reset");
    case "list_apps":
      return getRequest("/api/v1/apps");
    case "set_idle_threshold":
      return setIdleThresholdRequest(args);
    case "set_tracking_paused":
      return setTrackingPausedRequest(args);
    case "set_audio_participation":
      return setAudioParticipationRequest(args);
    case "configure_browser_activity":
      return configureBrowserActivityRequest(args);
    case "classify_app":
      return classifyAppRequest(args);
    case "rename_app":
      return renameAppRequest(args);
    case "set_app_excluded":
      return setAppExcludedRequest(args);
    default:
      return null;
  }
}

async function applyConnectionChange(name: string, payload: unknown, deps: McpDeps) {
  if (name === "set_local_api_port") {
    const baseUrl = nestedString(payload, ["data", "configuration", "base_url"]);
    if (baseUrl) deps.apiBase = baseUrl;
  }
  if (name === "rotate_local_api_token" && deps.refreshApiToken) {
    const tokenPath = nestedString(payload, ["data", "configuration", "token_path"]);
    deps.apiToken = await deps.refreshApiToken(tokenPath || undefined);
  }
}

function setLocalApiPortRequest(args: Record<string, unknown>) {
  const port = boundedInteger(args.port, 1024, 65535);
  if (port === null) {
    return { error: "set_local_api_port requires an integer port from 1024 through 65535" };
  }
  return postRequest("/api/v1/settings/local-api/port", { port });
}

function rotateLocalApiTokenRequest(args: Record<string, unknown>) {
  if (args.confirmed !== true) {
    return { error: "rotate_local_api_token requires confirmed=true after explicit user confirmation" };
  }
  return postRequest("/api/v1/settings/local-api/token/rotate");
}

function restartDaemonServiceRequest(args: Record<string, unknown>) {
  if (args.confirmed !== true) {
    return { error: "restart_daemon_service requires confirmed=true after explicit user confirmation" };
  }
  return postRequest("/api/v1/system/service/restart", { confirmed: true });
}

function createReminderRequest(args: Record<string, unknown>) {
  const label = optionalString(args.label, 256);
  const scheduledAt = integerValue(args.scheduledAt);
  if (label === null || scheduledAt === null || scheduledAt <= 0) {
    return { error: "create_reminder requires a label up to 256 characters and a future scheduledAt timestamp" };
  }
  return postRequest("/api/v1/tools/reminders", {
    label,
    scheduled_at: scheduledAt,
  });
}

function createSoftwareReminderRuleRequest(args: Record<string, unknown>) {
  const appName = optionalString(args.appName, 256)?.trim();
  const exeName = args.exeName === undefined || args.exeName === null
    ? null
    : optionalString(args.exeName, 256)?.trim();
  const limitMs = integerValue(args.limitMs);
  const message = optionalString(args.message, 1024);
  if (
    !appName
    || exeName === undefined
    || limitMs === null
    || limitMs < 60_000
    || limitMs > 86_400_000
    || message === null
  ) {
    return { error: "create_software_reminder_rule requires valid appName, optional exeName, limitMs, and message" };
  }
  return postRequest("/api/v1/tools/software-reminder-rules", {
    app_name: appName,
    exe_name: exeName || null,
    limit_ms: limitMs,
    message,
  });
}

function startTimerRequest(args: Record<string, unknown>) {
  const mode = stringValue(args.mode);
  const label = args.label === undefined || args.label === null
    ? null
    : optionalString(args.label, 256)?.trim();
  const durationMs = args.durationMs === undefined || args.durationMs === null
    ? null
    : integerValue(args.durationMs);
  if (
    !["stopwatch", "countdown"].includes(mode)
    || label === undefined
    || (mode === "countdown"
      && (durationMs === null || durationMs < 60_000 || durationMs > 10_800_000))
  ) {
    return { error: "start_timer requires mode and a 60000-10800000 durationMs for countdowns" };
  }
  return postRequest("/api/v1/tools/timer/start", {
    mode,
    duration_ms: mode === "countdown" ? durationMs : null,
    label: label || null,
  });
}

function startPomodoroRequest(args: Record<string, unknown>) {
  const focusMs = boundedInteger(args.focusMs, 60_000, 10_800_000);
  const shortBreakMs = boundedInteger(args.shortBreakMs, 60_000, 3_600_000);
  const longBreakMs = boundedInteger(args.longBreakMs, 60_000, 7_200_000);
  const longBreakEvery = boundedInteger(args.longBreakEvery, 2, 12);
  if ([focusMs, shortBreakMs, longBreakMs, longBreakEvery].some((value) => value === null)) {
    return { error: "start_pomodoro requires valid focus, break, and cycle durations" };
  }
  return postRequest("/api/v1/tools/pomodoro/start", {
    focus_ms: focusMs,
    short_break_ms: shortBreakMs,
    long_break_ms: longBreakMs,
    long_break_every: longBreakEvery,
  });
}

function idActionRequest(
  args: Record<string, unknown>,
  basePath: string,
  toolName: string,
  action: string,
) {
  const id = integerValue(args.id);
  if (id === null || id <= 0) {
    return { error: `${toolName} requires a positive integer id` };
  }
  return postRequest(`${basePath}/${id}/${action}`);
}

function postRequest(path: string, body?: unknown) {
  return {
    path,
    init: {
      method: "POST" as const,
      ...(body === undefined ? {} : { body }),
    },
  };
}

function setIdleThresholdRequest(args: Record<string, unknown>) {
  const seconds = numberValue(args.seconds);
  if (
    seconds === null
    || !Number.isInteger(seconds)
    || seconds < 60
    || seconds > 86400
  ) {
    return { error: "set_idle_threshold requires integer seconds from 60 through 86400" };
  }
  return {
    path: "/api/v1/settings/tracker/afk-threshold",
    init: {
      method: "POST" as const,
      body: { seconds },
    },
  };
}

function setTrackingPausedRequest(args: Record<string, unknown>) {
  const paused = booleanValue(args.paused);
  if (paused === null) {
    return { error: "set_tracking_paused requires paused" };
  }
  return {
    path: "/api/v1/settings/tracker/pause",
    init: {
      method: "POST" as const,
      body: { paused },
    },
  };
}

function setAudioParticipationRequest(args: Record<string, unknown>) {
  const enabled = booleanValue(args.enabled);
  if (enabled === null) {
    return { error: "set_audio_participation requires enabled" };
  }
  return {
    path: "/api/v1/settings/runtime/audio-participation",
    init: {
      method: "POST" as const,
      body: { enabled },
    },
  };
}

function configureBrowserActivityRequest(args: Record<string, unknown>) {
  const enabled = booleanValue(args.enabled);
  const port = numberValue(args.port);
  const token = stringValue(args.token);
  const urlPrivacy = stringValue(args.urlPrivacy);
  if (
    enabled === null
    || port === null
    || !Number.isInteger(port)
    || port < 1024
    || port > 65535
    || token === null
    || token.length > 512
    || (enabled && token.trim().length === 0)
    || !["full", "strip_query", "domain_only"].includes(urlPrivacy ?? "")
  ) {
    return {
      error: "configure_browser_activity requires enabled, port 1024-65535, a valid token, and urlPrivacy",
    };
  }
  return {
    path: "/api/v1/settings/runtime/browser-activity",
    init: {
      method: "POST" as const,
      body: {
        enabled,
        port,
        token: token.trim(),
        url_privacy: urlPrivacy,
      },
    },
  };
}

function renameAppRequest(args: Record<string, unknown>) {
  const exeName = stringValue(args.exeName);
  const displayName = stringValue(args.displayName);
  if (!exeName) {
    return { error: "rename_app requires exeName" };
  }
  if (!displayName) {
    return { error: "rename_app requires displayName" };
  }

  return {
    path: `/api/v1/apps/${encodeURIComponent(exeName)}/rename`,
    init: {
      method: "POST" as const,
      body: { display_name: displayName },
    },
  };
}

function setAppExcludedRequest(args: Record<string, unknown>) {
  const exeName = stringValue(args.exeName);
  const excluded = booleanValue(args.excluded);
  if (!exeName) {
    return { error: "set_app_excluded requires exeName" };
  }
  if (excluded === null) {
    return { error: "set_app_excluded requires excluded" };
  }

  return {
    path: `/api/v1/apps/${encodeURIComponent(exeName)}/exclude`,
    init: {
      method: "POST" as const,
      body: { excluded },
    },
  };
}

function getRequest(path: string) {
  return { path };
}

function classifyAppRequest(args: Record<string, unknown>) {
  const exeName = stringValue(args.exeName);
  const category = stringValue(args.category);
  if (!exeName) {
    return { error: "classify_app requires exeName" };
  }
  if (!category) {
    return { error: "classify_app requires category" };
  }

  return {
    path: `/api/v1/apps/${encodeURIComponent(exeName)}/classify`,
    init: {
      method: "POST" as const,
      body: { category },
    },
  };
}

function sessionsPath(args: Record<string, unknown>) {
  const params = new URLSearchParams();
  appendNumberParam(params, "from", args.from);
  appendNumberParam(params, "to", args.to);
  appendStringParam(params, "app", args.app);
  appendNumberParam(params, "limit", args.limit);
  const query = params.toString();
  return query ? `/api/v1/sessions?${query}` : "/api/v1/sessions";
}

function trendPath(args: Record<string, unknown>) {
  const params = new URLSearchParams();
  appendStringParam(params, "period", args.period);
  appendStringParam(params, "granularity", args.granularity);
  const query = params.toString();
  return query ? `/api/v1/trend?${query}` : "/api/v1/trend";
}

function webActivityPath(args: Record<string, unknown>) {
  const params = new URLSearchParams();
  appendNumberParam(params, "from", args.from);
  appendNumberParam(params, "to", args.to);
  appendStringParam(params, "domain", args.domain);
  appendNumberParam(params, "limit", args.limit);
  const query = params.toString();
  return query ? `/api/v1/web-activity?${query}` : "/api/v1/web-activity";
}

function appendNumberParam(params: URLSearchParams, key: string, value: unknown) {
  if (typeof value === "number" && Number.isFinite(value)) {
    params.set(key, String(value));
  }
}

function appendStringParam(params: URLSearchParams, key: string, value: unknown) {
  const normalized = stringValue(value);
  if (normalized) {
    params.set(key, normalized);
  }
}

function stringValue(value: unknown) {
  return typeof value === "string" ? value.trim() : "";
}

function nestedString(value: unknown, path: string[]) {
  let current: unknown = value;
  for (const key of path) {
    if (!current || typeof current !== "object" || Array.isArray(current)) return "";
    current = (current as Record<string, unknown>)[key];
  }
  return typeof current === "string" ? current.trim() : "";
}

function booleanValue(value: unknown) {
  return typeof value === "boolean" ? value : null;
}

function numberValue(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function integerValue(value: unknown) {
  const number = numberValue(value);
  return number !== null && Number.isSafeInteger(number) ? number : null;
}

function boundedInteger(value: unknown, minimum: number, maximum: number) {
  const number = integerValue(value);
  return number !== null && number >= minimum && number <= maximum ? number : null;
}

function optionalString(value: unknown, maxLength: number) {
  return typeof value === "string" && value.length <= maxLength ? value : null;
}

function ok(id: string | number | null, result: Record<string, unknown>): JsonRpcResponse {
  return {
    jsonrpc: "2.0",
    id,
    result,
  };
}

function error(id: string | number | null, code: number, message: string): JsonRpcResponse {
  return {
    jsonrpc: "2.0",
    id,
    error: {
      code,
      message,
    },
  };
}

function objectSchema(properties: Record<string, unknown>, required: string[] = []) {
  return {
    type: "object",
    properties,
    ...(required.length > 0 ? { required } : {}),
    additionalProperties: false,
  };
}

function positiveIdSchema(description: string) {
  return objectSchema({
    id: { type: "integer", minimum: 1, description },
  }, ["id"]);
}

function toolsActionMetadata(): PatinaMcpTool[] {
  return [
    ["pause_timer", "Pause the current Patina timer."],
    ["resume_timer", "Resume the current Patina timer."],
    ["reset_timer", "Reset the current Patina timer."],
    ["add_timer_lap", "Add a lap to the running Patina stopwatch."],
  ].map(([name, description]) => ({
    name,
    description: `${description} This changes local Tools state.`,
    inputSchema: objectSchema({}),
  }));
}

function pomodoroActionMetadata(): PatinaMcpTool[] {
  return [
    ["pause_pomodoro", "Pause the current Patina Pomodoro run."],
    ["resume_pomodoro", "Resume the current Patina Pomodoro run."],
    ["skip_pomodoro_phase", "Skip the current Patina Pomodoro phase."],
    ["reset_pomodoro", "Reset the current Patina Pomodoro run."],
  ].map(([name, description]) => ({
    name,
    description: `${description} This changes local Tools state.`,
    inputSchema: objectSchema({}),
  }));
}

function stringField(params: Record<string, unknown> | undefined, field: string) {
  const value = params?.[field];
  return typeof value === "string" ? value : null;
}

function objectField(params: Record<string, unknown> | undefined, field: string) {
  const value = params?.[field];
  if (value && typeof value === "object" && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }
  return {};
}

async function readApiToken() {
  if (process.env.PATINA_API_TOKEN?.trim()) {
    return process.env.PATINA_API_TOKEN.trim();
  }

  const tokenPath = process.env.PATINA_API_TOKEN_FILE?.trim() || defaultTokenPath();
  return (await readFile(tokenPath, "utf8")).trim();
}

async function readApiTokenFile(tokenPath?: string) {
  const resolvedPath = tokenPath
    || process.env.PATINA_API_TOKEN_FILE?.trim()
    || defaultTokenPath();
  return (await readFile(resolvedPath, "utf8")).trim();
}

function defaultTokenPath() {
  const dataHome = process.env.XDG_DATA_HOME?.trim()
    || join(process.env.HOME || ".", ".local", "share");
  return join(dataHome, "Patina", "api_token");
}

async function main() {
  const deps: McpDeps = {
    apiBase: process.env.PATINA_API_BASE?.trim() || DEFAULT_API_BASE,
    apiToken: await readApiToken(),
    refreshApiToken: readApiTokenFile,
    callApi: callPatinaApi,
  };

  const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
  for await (const line of lines) {
    if (!line.trim()) continue;

    const responseLine = await handleMcpLine(line, deps);
    if (responseLine) {
      process.stdout.write(`${responseLine}\n`);
    }
  }
}

if (resolve(process.argv[1] || "") === fileURLToPath(import.meta.url)) {
  await main();
}
