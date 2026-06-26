import { readFile } from "node:fs/promises";
import { join } from "node:path";
import process from "node:process";
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
    name: "list_apps",
    description: "List known apps from recorded Patina sessions.",
    inputSchema: objectSchema({}),
  },
  {
    name: "classify_app",
    description: "Assign a Patina category to an app exe_name.",
    inputSchema: objectSchema({
      exeName: { type: "string", description: "Exact app exe_name to classify." },
      category: { type: "string", description: "Category name to assign." },
    }),
  },
  {
    name: "rename_app",
    description: "Assign a Patina display name to an app exe_name.",
    inputSchema: objectSchema({
      exeName: { type: "string", description: "Exact app exe_name to rename." },
      displayName: { type: "string", description: "Display name to assign." },
    }),
  },
  {
    name: "set_app_excluded",
    description: "Set whether an app is excluded from Patina activity statistics.",
    inputSchema: objectSchema({
      exeName: { type: "string", description: "Exact app exe_name to update." },
      excluded: { type: "boolean", description: "Whether the app should be excluded." },
    }),
  },
];

export async function handleMcpRequest(
  request: JsonRpcRequest,
  deps: McpDeps,
): Promise<JsonRpcResponse> {
  const id = request.id ?? null;

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
      return ok(id, {
        content: [
          {
            type: "text",
            text: JSON.stringify(payload, null, 2),
          },
        ],
      });
    } catch (apiError) {
      return error(id, -32000, apiError instanceof Error ? apiError.message : String(apiError));
    }
  }

  return error(id, -32601, `Unsupported method: ${request.method ?? "unknown"}`);
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
    case "list_apps":
      return getRequest("/api/v1/apps");
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

function booleanValue(value: unknown) {
  return typeof value === "boolean" ? value : null;
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

function objectSchema(properties: Record<string, unknown>) {
  return {
    type: "object",
    properties,
    additionalProperties: false,
  };
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

function defaultTokenPath() {
  const dataHome = process.env.XDG_DATA_HOME?.trim()
    || join(process.env.HOME || ".", ".local", "share");
  return join(dataHome, "Patina", "api_token");
}

function encodeMcpMessage(message: unknown) {
  const json = JSON.stringify(message);
  return `Content-Length: ${Buffer.byteLength(json, "utf8")}\r\n\r\n${json}`;
}

function decodeMcpMessages(buffer: string) {
  const messages: unknown[] = [];
  let cursor = 0;

  while (cursor < buffer.length) {
    const headerEnd = buffer.indexOf("\r\n\r\n", cursor);
    if (headerEnd === -1) break;
    const header = buffer.slice(cursor, headerEnd);
    const lengthMatch = /Content-Length:\s*(\d+)/i.exec(header);
    if (!lengthMatch) break;
    const bodyStart = headerEnd + 4;
    const bodyLength = Number(lengthMatch[1]);
    const bodyEnd = bodyStart + bodyLength;
    if (bodyEnd > buffer.length) break;
    messages.push(JSON.parse(buffer.slice(bodyStart, bodyEnd)));
    cursor = bodyEnd;
  }

  return messages;
}

async function main() {
  const deps: McpDeps = {
    apiBase: process.env.PATINA_API_BASE?.trim() || DEFAULT_API_BASE,
    apiToken: await readApiToken(),
    callApi: callPatinaApi,
  };

  let input = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk) => {
    input += chunk;
    void Promise.resolve().then(async () => {
      for (const message of decodeMcpMessages(input)) {
        const response = await handleMcpRequest(message as JsonRpcRequest, deps);
        process.stdout.write(encodeMcpMessage(response));
      }
      input = "";
    });
  });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  await main();
}
