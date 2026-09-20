import {
  deleteWebDavBackupSecret,
  hasWebDavBackupSecret,
  listWebDavBackups,
  revealWebDavBackupSecret,
  restoreWebDavBackup,
  saveWebDavBackupSecret,
  testWebDavBackupTarget,
  uploadWebDavBackup,
  type RemoteBackupEntry,
  type WebDavBackupConfig,
} from "../../../platform/backup/remoteBackupRuntimeGateway.ts";
import {
  clearRemoteBackupConfig,
  DEFAULT_WEBDAV_REMOTE_DIR,
  loadRemoteBackupConfig,
  saveRemoteBackupConfig,
  type PersistedRemoteBackupConfig,
} from "../../../platform/persistence/remoteBackupSettingsStore.ts";
import type { BackupRestoreStrategy } from "./settingsRuntimeAdapterService.ts";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";

export { DEFAULT_WEBDAV_REMOTE_DIR };
export type { PersistedRemoteBackupConfig, RemoteBackupEntry };

export interface RemoteBackupFormDraft {
  url: string;
  username: string;
  remoteDir: string;
  password: string;
}

interface SaveRemoteBackupOptions {
  config: PersistedRemoteBackupConfig | null;
  hasSecret: boolean;
  onSecretPresenceChange: (hasSecret: boolean) => void;
}

const defaultDeps = {
  loadRemoteBackupConfig,
  hasWebDavBackupSecret,
  saveWebDavBackupSecret,
  saveRemoteBackupConfig,
  clearRemoteBackupConfig,
  deleteWebDavBackupSecret,
  revealWebDavBackupSecret,
  testWebDavBackupTarget,
  uploadWebDavBackup,
  listWebDavBackups,
  restoreWebDavBackup,
  reportError: (message: string, error: unknown) => console.error(message, error),
};

function toRuntimeConfig(config: PersistedRemoteBackupConfig | RemoteBackupFormDraft): WebDavBackupConfig {
  if ("password" in config) {
    return {
      url: config.url.trim(),
      username: config.username.trim(),
      remoteDir: config.remoteDir.trim() || DEFAULT_WEBDAV_REMOTE_DIR,
    };
  }
  return { url: config.url, username: config.username, remoteDir: config.remoteDir };
}

export function buildRemoteBackupSummary(entry: RemoteBackupEntry): string {
  return [
    `${UI_TEXT.backup.versionLabel(entry.backupVersion)}（${UI_TEXT.backup.schemaLabel(entry.schemaVersion)}）`,
    UI_TEXT.backup.exportedAt(new Date(entry.createdAtMs).toLocaleString()),
    UI_TEXT.backup.appVersion(entry.appVersion),
    UI_TEXT.backup.itemCounts(entry.sessionCount, entry.settingCount, entry.iconCacheCount),
  ].join("\n");
}

export function createSettingsRemoteBackupService(deps = defaultDeps) {
  return {
    async load() {
      const [config, hasSecret] = await Promise.all([
        deps.loadRemoteBackupConfig(),
        deps.hasWebDavBackupSecret(),
      ]);
      return { config, hasSecret };
    },

    async saveConfig(draft: RemoteBackupFormDraft, options: SaveRemoteBackupOptions) {
      let savedNewSecretForUnsavedConfig = false;
      try {
        const runtimeConfig = toRuntimeConfig(draft);
        const password = draft.password.trim();
        if (password) {
          await deps.saveWebDavBackupSecret(runtimeConfig.username, password);
          savedNewSecretForUnsavedConfig = !options.config;
          options.onSecretPresenceChange(true);
        } else if (!options.config || !options.hasSecret) {
          return null;
        }
        return await deps.saveRemoteBackupConfig({
          ...runtimeConfig,
          lastBackupAtMs: options.config?.lastBackupAtMs ?? null,
        });
      } catch (error) {
        deps.reportError("save WebDAV backup config failed", error);
        // Only a newly configured target owns an orphan secret to roll back.
        if (savedNewSecretForUnsavedConfig) {
          try {
            await deps.deleteWebDavBackupSecret();
            options.onSecretPresenceChange(false);
          } catch (deleteError) {
            deps.reportError("rollback unsaved WebDAV secret failed", deleteError);
          }
        }
        throw error;
      }
    },

    async deleteConfig() {
      await deps.clearRemoteBackupConfig();
      await deps.deleteWebDavBackupSecret();
    },
    deleteUnsavedSecret: deps.deleteWebDavBackupSecret,
    revealSavedPassword: deps.revealWebDavBackupSecret,
    testConfig(config: PersistedRemoteBackupConfig | RemoteBackupFormDraft) {
      const password = "password" in config ? config.password.trim() || undefined : undefined;
      return deps.testWebDavBackupTarget(toRuntimeConfig(config), password);
    },
    async uploadBackup(config: PersistedRemoteBackupConfig) {
      const result = await deps.uploadWebDavBackup(toRuntimeConfig(config));
      return { ...result, config: { ...config, lastBackupAtMs: result.entry.createdAtMs } };
    },
    listBackups(config: PersistedRemoteBackupConfig) {
      return deps.listWebDavBackups(toRuntimeConfig(config));
    },
    restoreBackup(config: PersistedRemoteBackupConfig, id: string, strategy: BackupRestoreStrategy) {
      return deps.restoreWebDavBackup(toRuntimeConfig(config), id, strategy);
    },
  };
}

export const SettingsRemoteBackupService = createSettingsRemoteBackupService();
