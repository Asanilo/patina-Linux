import { getAppVersion } from "../../../platform/desktop/appInfoGateway.ts";
import {
  loadAppSettingsSnapshot,
  type AppSettings,
} from "../../../platform/persistence/appSettingsStore.ts";
import {
  getSettingsBootstrapCache,
  setSettingsBootstrapCache,
} from "./settingsBootstrapCache.ts";

export interface SettingsPageBootstrapData {
  settings: AppSettings;
  appVersion: string;
  productRevision?: string;
  resourceRevision?: string | null;
}

type SettingsPageBootstrapDeps = {
  getAppVersion: () => Promise<string>;
  loadAppSettingsSnapshot: typeof loadAppSettingsSnapshot;
};

const settingsPageBootstrapDeps: SettingsPageBootstrapDeps = {
  getAppVersion: async () => getAppVersion().catch(() => "unknown"),
  loadAppSettingsSnapshot,
};

export async function loadSettingsPageBootstrapWithDeps(
  deps: SettingsPageBootstrapDeps,
): Promise<SettingsPageBootstrapData> {
  const [snapshot, appVersion, localApiSettings] = await Promise.all([
    deps.loadAppSettingsSnapshot(),
    deps.getAppVersion(),
    loadLocalApiSettingsForBootstrap(),
  ]);
  const settings = snapshot.settings;
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
    productRevision: snapshot.productRevision,
    resourceRevision: snapshot.resourceRevision,
  };
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
  const bootstrap = await loadSettingsPageBootstrap();
  setSettingsBootstrapCache(bootstrap);
  return bootstrap;
}

export { onAppSettingsChanged as subscribeSettingsChanges } from "../../../platform/runtime/appSettingsEventGateway.ts";
