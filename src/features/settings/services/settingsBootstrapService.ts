import { getAppVersion } from "../../../platform/desktop/appInfoGateway.ts";
import {
  loadAppSettings,
  type AppSettings,
} from "../../../platform/persistence/appSettingsStore.ts";
import {
  getSettingsBootstrapCache,
  setSettingsBootstrapCache,
} from "./settingsBootstrapCache.ts";

export interface SettingsPageBootstrapData {
  settings: AppSettings;
  appVersion: string;
}

type SettingsPageBootstrapDeps = {
  getAppVersion: () => Promise<string>;
  loadAppSettings: () => Promise<AppSettings>;
  setSettingsBootstrapCache: (bootstrap: SettingsPageBootstrapData) => void;
};

const settingsPageBootstrapDeps: SettingsPageBootstrapDeps = {
  getAppVersion: async () => getAppVersion().catch(() => "unknown"),
  loadAppSettings,
  setSettingsBootstrapCache,
};

export async function loadSettingsPageBootstrapWithDeps(
  deps: SettingsPageBootstrapDeps,
): Promise<SettingsPageBootstrapData> {
  const [settings, appVersion, localApiSettings] = await Promise.all([
    deps.loadAppSettings(),
    deps.getAppVersion(),
    loadLocalApiSettingsForBootstrap(),
  ]);
  const mergedSettings = localApiSettings
    ? {
        ...settings,
        localApiPort: localApiSettings.port,
        localApiToken: localApiSettings.token,
      }
    : settings;

  const bootstrap = {
    settings: mergedSettings,
    appVersion,
  };
  deps.setSettingsBootstrapCache(bootstrap);
  return bootstrap;
}

async function loadLocalApiSettingsForBootstrap() {
  try {
    const module = await import("../../../platform/runtime/localApiDiagnosticsGateway.ts");
    return await module.getLocalApiSettings();
  } catch {
    return null;
  }
}

export async function loadSettingsPageBootstrap(): Promise<SettingsPageBootstrapData> {
  return loadSettingsPageBootstrapWithDeps(settingsPageBootstrapDeps);
}

export function getSettingsPageBootstrapCache(): SettingsPageBootstrapData | null {
  return getSettingsBootstrapCache();
}

export async function prewarmSettingsBootstrapCache(): Promise<SettingsPageBootstrapData> {
  return loadSettingsPageBootstrap();
}
