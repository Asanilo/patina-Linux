import assert from "node:assert/strict";
import {
  createSettingsDiagnosticsService,
  type WebActivityBridgeSnapshot,
  type DesktopIntegrationDiagnosticsSnapshot,
} from "../src/features/settings/services/settingsDiagnosticsService.ts";
import {
  createSettingsRemoteBackupService,
  DEFAULT_WEBDAV_REMOTE_DIR,
  type PersistedRemoteBackupConfig,
  type RemoteBackupFormDraft,
} from "../src/features/settings/services/settingsRemoteBackupService.ts";

let passed = 0;
async function runTest(name: string, run: () => Promise<void>) {
  await run();
  passed += 1;
  console.log(`PASS ${name}`);
}

async function unexpected(): Promise<never> {
  throw new Error("Unexpected platform operation");
}

const diagnosticsDeps: NonNullable<Parameters<typeof createSettingsDiagnosticsService>[0]> = {
  getWebActivityBridgeSnapshot: unexpected,
  getLocalApiDiagnostics: unexpected,
  getDesktopIntegrationDiagnostics: unexpected,
  getDaemonServiceDiagnostics: unexpected,
  repairAutostartDesktopFile: unexpected,
  reloadDaemonVersion: unexpected,
  retryRuntimeOwnerCutover: unexpected,
  rollbackRuntimeOwnerToEmbedded: unexpected,
  setBackgroundTrackingAtLogin: unexpected,
  reportWarning: () => {},
};

const remoteBackupDeps: NonNullable<Parameters<typeof createSettingsRemoteBackupService>[0]> = {
  loadRemoteBackupConfig: unexpected,
  hasWebDavBackupSecret: unexpected,
  saveWebDavBackupSecret: unexpected,
  saveRemoteBackupConfig: unexpected,
  clearRemoteBackupConfig: unexpected,
  deleteWebDavBackupSecret: unexpected,
  revealWebDavBackupSecret: unexpected,
  testWebDavBackupTarget: unexpected,
  uploadWebDavBackup: unexpected,
  listWebDavBackups: unexpected,
  restoreWebDavBackup: unexpected,
  reportError: () => {},
};

const draft: RemoteBackupFormDraft = {
  url: " https://backup.example.test/dav ",
  username: " alice ",
  remoteDir: " ",
  password: " secret ",
};
const config: PersistedRemoteBackupConfig = {
  url: "https://backup.example.test/dav",
  username: "alice",
  remoteDir: DEFAULT_WEBDAV_REMOTE_DIR,
  lastBackupAtMs: 12345,
};

await runTest("one unavailable diagnostic preserves the other live snapshots", async () => {
  const bridge: WebActivityBridgeSnapshot = {
    enabled: true, listening: true, connected: false, browserClientId: null,
    browserKind: null, extensionVersion: null, lastActivityAtMs: null,
  };
  const desktop: DesktopIntegrationDiagnosticsSnapshot = {
    launchAtLogin: false, backgroundTrackingAtLogin: true, startMinimized: false,
    autostart: { path: "/tmp/patina.desktop", exists: false, exec: null, valid: false, reason: "absent" },
  };
  const failure = new Error("API unavailable");
  const warnings: unknown[][] = [];
  const service = createSettingsDiagnosticsService({
    ...diagnosticsDeps,
    getWebActivityBridgeSnapshot: async () => bridge,
    getLocalApiDiagnostics: async () => { throw failure; },
    getDesktopIntegrationDiagnostics: async () => desktop,
    reportWarning: (...args) => warnings.push(args),
  });
  assert.deepEqual(await service.loadLive(), { bridge, localApi: null, desktopIntegration: desktop });
  assert.deepEqual(warnings, [["load local API diagnostics failed", failure]]);
});

await runTest("diagnostics clear all stale rows when every independent source fails", async () => {
  const warnings: unknown[][] = [];
  const service = createSettingsDiagnosticsService({
    ...diagnosticsDeps,
    reportWarning: (...args) => warnings.push(args),
  });
  assert.deepEqual(await service.loadLive(), { bridge: null, localApi: null, desktopIntegration: null });
  assert.equal(warnings.length, 3);
});

await runTest("new backup configuration needs a password before any platform writes", async () => {
  const service = createSettingsRemoteBackupService(remoteBackupDeps);
  assert.equal(await service.saveConfig({ ...draft, password: " " }, {
    config: null, hasSecret: true, onSecretPresenceChange: () => assert.fail("Secret must not change"),
  }), null);
});

await runTest("configuration save records the secret before settings and preserves backup history", async () => {
  const events: unknown[] = [];
  const service = createSettingsRemoteBackupService({
    ...remoteBackupDeps,
    saveWebDavBackupSecret: async (...args) => { events.push(["secret", ...args]); },
    saveRemoteBackupConfig: async (next) => { events.push(["config", next]); return config; },
  });
  assert.equal(await service.saveConfig(draft, {
    config, hasSecret: false, onSecretPresenceChange: (present) => events.push(["presence", present]),
  }), config);
  assert.deepEqual(events, [
    ["secret", "alice", "secret"],
    ["presence", true],
    ["config", config],
  ]);
});

await runTest("saved backup target reuses its secret when the password field is empty", async () => {
  const service = createSettingsRemoteBackupService({
    ...remoteBackupDeps,
    saveRemoteBackupConfig: async (next) => {
      assert.deepEqual(next, config);
      return config;
    },
  });
  assert.equal(await service.saveConfig({ ...draft, password: " " }, {
    config, hasSecret: true, onSecretPresenceChange: () => assert.fail("Secret must not change"),
  }), config);
});

await runTest("failed first configuration save removes only its newly created secret", async () => {
  const failure = new Error("Settings write failed");
  const events: unknown[] = [];
  const service = createSettingsRemoteBackupService({
    ...remoteBackupDeps,
    saveWebDavBackupSecret: async () => { events.push("save-secret"); },
    saveRemoteBackupConfig: async (next) => {
      assert.equal(next.lastBackupAtMs, null);
      events.push("save-config");
      throw failure;
    },
    deleteWebDavBackupSecret: async () => { events.push("delete-secret"); },
  });
  await assert.rejects(service.saveConfig(draft, {
    config: null, hasSecret: false, onSecretPresenceChange: (present) => events.push(present),
  }), (error) => error === failure);
  assert.deepEqual(events, ["save-secret", true, "save-config", "delete-secret", false]);
});

await runTest("rollback failure retains secret presence and reports the original save failure", async () => {
  const failure = new Error("Settings write failed");
  const rollbackFailure = new Error("Keyring unavailable");
  const presence: boolean[] = [];
  const errors: unknown[][] = [];
  const service = createSettingsRemoteBackupService({
    ...remoteBackupDeps,
    saveWebDavBackupSecret: async () => {},
    saveRemoteBackupConfig: async () => { throw failure; },
    deleteWebDavBackupSecret: async () => { throw rollbackFailure; },
    reportError: (...args) => errors.push(args),
  });
  await assert.rejects(service.saveConfig(draft, {
    config: null, hasSecret: false, onSecretPresenceChange: (present) => presence.push(present),
  }), (error) => error === failure);
  assert.deepEqual(presence, [true]);
  assert.deepEqual(errors, [
    ["save WebDAV backup config failed", failure],
    ["rollback unsaved WebDAV secret failed", rollbackFailure],
  ]);
});

await runTest("failed existing configuration save never deletes the existing target secret", async () => {
  const failure = new Error("Settings write failed");
  const service = createSettingsRemoteBackupService({
    ...remoteBackupDeps,
    saveWebDavBackupSecret: async () => {},
    saveRemoteBackupConfig: async () => { throw failure; },
    deleteWebDavBackupSecret: async () => { assert.fail("Existing target secret must remain"); },
  });
  await assert.rejects(service.saveConfig(draft, {
    config, hasSecret: true, onSecretPresenceChange: () => {},
  }), (error) => error === failure);
});

await runTest("backup deletion clears settings before deleting the secret", async () => {
  const events: string[] = [];
  const service = createSettingsRemoteBackupService({
    ...remoteBackupDeps,
    clearRemoteBackupConfig: async () => { events.push("config"); },
    deleteWebDavBackupSecret: async () => { events.push("secret"); },
  });
  await service.deleteConfig();
  assert.deepEqual(events, ["config", "secret"]);

  const failure = new Error("Settings unavailable");
  const failingService = createSettingsRemoteBackupService({
    ...remoteBackupDeps,
    clearRemoteBackupConfig: async () => { throw failure; },
    deleteWebDavBackupSecret: async () => { assert.fail("Secret must remain when clear fails"); },
  });
  await assert.rejects(failingService.deleteConfig(), (error) => error === failure);
});

await runTest("connection test normalizes drafts without exposing saved backup metadata", async () => {
  const targets: unknown[][] = [];
  const service = createSettingsRemoteBackupService({
    ...remoteBackupDeps,
    testWebDavBackupTarget: async (...args) => { targets.push(args); return true; },
  });
  assert.equal(await service.testConfig(draft), true);
  assert.equal(await service.testConfig(config), true);
  const target = { url: config.url, username: config.username, remoteDir: config.remoteDir };
  assert.deepEqual(targets, [[target, "secret"], [target, undefined]]);
});

console.log(`Passed ${passed} settings service boundary tests`);
