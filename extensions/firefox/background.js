const chrome = browser;
const PROTOCOL_VERSION = 1;
const EXTENSION_VERSION = chrome.runtime.getManifest().version;
const DEFAULT_PORT = "12345";
const PORT_PATTERN = /^\d{1,5}$/;
const FAVICON_DATA_URL_MAX_CHARS = 8192;
const STORAGE_DEFAULTS = {
  enabled: true,
  port: DEFAULT_PORT,
  token: "",
  clientId: "",
  lastStatus: "disabled",
  lastMessage: "",
  lastSeenAt: 0,
};

let pendingActiveTabTimer = null;

async function browserKind() {
  let identity = navigator.userAgent.toLowerCase();
  if (typeof chrome.runtime.getBrowserInfo === "function") {
    try {
      const info = await chrome.runtime.getBrowserInfo();
      identity = `${info?.name || ""} ${info?.vendor || ""} ${identity}`.toLowerCase();
    } catch {
      // User-agent fallback remains available on Firefox-compatible forks.
    }
  }
  if (identity.includes("zen")) return "zen";
  if (identity.includes("floorp")) return "floorp";
  if (identity.includes("iceweasel")) return "iceweasel";
  if (identity.includes("librewolf")) return "librewolf";
  if (identity.includes("firefox")) return "firefox";
  if (identity.includes("edg/")) return "edge";
  if (identity.includes("opr/") || identity.includes("opera")) return "opera";
  if (identity.includes("vivaldi")) return "vivaldi";
  if (identity.includes("brave")) return "brave";
  return "chrome";
}

function setStatus(lastStatus, lastMessage = "") {
  return chrome.storage.local.set({
    lastStatus,
    lastMessage,
    lastSeenAt: Date.now(),
  });
}

async function getSettings() {
  const settings = await chrome.storage.local.get(STORAGE_DEFAULTS);
  let clientId = String(settings.clientId || "").trim();
  const storagePatch = {};
  if (!clientId) {
    clientId = crypto.randomUUID();
    storagePatch.clientId = clientId;
  }
  if (settings.enabled !== true) {
    storagePatch.enabled = true;
  }
  if (Object.keys(storagePatch).length > 0) {
    await chrome.storage.local.set(storagePatch);
  }
  const port = normalizePort(settings.port);
  return {
    ...STORAGE_DEFAULTS,
    ...settings,
    clientId,
    port,
    token: String(settings.token || "").trim(),
    enabled: true,
  };
}

function normalizePort(rawPort, fallback = DEFAULT_PORT) {
  const value = String(rawPort || "").trim();
  if (!PORT_PATTERN.test(value)) return fallback;
  const port = Number(value);
  if (!Number.isInteger(port) || port < 1024 || port > 65535) return fallback;
  return String(port);
}

function endpointFromPort(port) {
  return `http://127.0.0.1:${port}`;
}

function webActivityUrl(endpoint) {
  const url = new URL(endpoint);
  if (!url.pathname || url.pathname === "/") {
    url.pathname = "/web-activity";
  }
  return url.toString();
}

function isTrackableTab(tab) {
  const url = String(tab?.url || "");
  return url.startsWith("http://") || url.startsWith("https://");
}

async function getActiveTrackableTab(eventReason) {
  const activeTabs = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  const activeTab = activeTabs[0];
  if (isTrackableTab(activeTab)) return activeTab;
  if (eventReason !== "manual") return null;

  const tabs = await chrome.tabs.query({ lastFocusedWindow: true });
  return tabs
    .filter(isTrackableTab)
    .sort((left, right) => (right.lastAccessed || 0) - (left.lastAccessed || 0))[0] || null;
}

async function resolveFaviconSource(tab) {
  const raw = String(tab?.favIconUrl || "").trim();
  if (raw.startsWith("data:")) {
    return raw.length <= FAVICON_DATA_URL_MAX_CHARS ? raw : undefined;
  }
  return raw || undefined;
}

async function sendActiveTab(eventReason = "refresh") {
  const settings = await getSettings();
  if (!settings.enabled) {
    await setStatus("disabled");
    return;
  }
  if (!settings.port || !settings.token) {
    await setStatus("needs-config", "请填写端口和 Token。");
    return;
  }

  const tab = await getActiveTrackableTab(eventReason);
  if (!tab) {
    await setStatus("disconnected", "当前没有可同步的网页。");
    return;
  }

  await setStatus("connecting");
  const favIconUrl = await resolveFaviconSource(tab);
  const payload = {
    protocolVersion: PROTOCOL_VERSION,
    browserClientId: settings.clientId,
    browserKind: await browserKind(),
    extensionVersion: EXTENSION_VERSION,
    tabId: tab.id,
    windowId: tab.windowId,
    url: tab.url,
    title: tab.title,
    favIconUrl,
    incognito: tab.incognito,
    capturedAtMs: Date.now(),
    eventReason,
  };

  try {
    const response = await fetch(webActivityUrl(endpointFromPort(settings.port)), {
      method: "POST",
      headers: {
        "Authorization": `Bearer ${settings.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(payload),
      cache: "no-store",
    });
    const data = await response.json().catch(() => null);
    if (data?.enabled === false) {
      await setStatus("disabled", "Patina 网页同步未开启。");
      return;
    }
    if (!response.ok || data?.ok === false) {
      await setStatus("error", data?.message || "");
      return;
    }
    await setStatus("connected");
  } catch {
    await setStatus("error");
  }
}

function queueActiveTab(eventReason) {
  if (pendingActiveTabTimer) clearTimeout(pendingActiveTabTimer);
  pendingActiveTabTimer = setTimeout(() => {
    pendingActiveTabTimer = null;
    void sendActiveTab(eventReason);
  }, 200);
}

chrome.runtime.onInstalled.addListener(() => {
  void getSettings().then(() => queueActiveTab("installed"));
  chrome.alarms.create("patina-active-tab-sync", { periodInMinutes: 0.5 });
});

chrome.runtime.onStartup.addListener(() => {
  queueActiveTab("startup");
  chrome.alarms.create("patina-active-tab-sync", { periodInMinutes: 0.5 });
});

chrome.tabs.onActivated.addListener(() => queueActiveTab("tab-activated"));
chrome.windows.onFocusChanged.addListener((windowId) => {
  if (windowId !== chrome.windows.WINDOW_ID_NONE) queueActiveTab("window-focused");
});
chrome.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (!tab.active) return;
  if (changeInfo.url || changeInfo.title || changeInfo.status === "complete" || changeInfo.favIconUrl) {
    queueActiveTab("tab-updated");
  }
});

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name !== "patina-active-tab-sync") return;
  queueActiveTab("periodic");
});

chrome.storage.onChanged.addListener((changes, areaName) => {
  if (areaName !== "local") return;
  if (changes.enabled?.newValue === true) {
    queueActiveTab("settings-enabled");
  }
  if (changes.enabled?.newValue === false) {
    void setStatus("disabled");
  }
});

chrome.runtime.onMessage.addListener((message) => {
  if (message?.type === "patina-connect-now" || message?.type === "patina-send-active-tab") {
    return sendActiveTab("manual").then(() => ({ ok: true }));
  }
  return undefined;
});

queueActiveTab("startup");
