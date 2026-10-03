import assert from "node:assert/strict";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { parseProductSettingsSnapshot, loadProductSettingsSnapshot } from "../src/platform/persistence/productSettingsSnapshot.ts";
import { loadAppSettings, loadTrackerHealthTimestamp } from "../src/platform/persistence/appSettingsStore.ts";
import { rebaseSettingsDraft, hasSettingsDraftPolicyConflict, hasSettingsDraftPolicyEdits } from "../src/features/settings/services/settingsDraftRebase.ts";
import { saveAppSettingsPatch } from "../src/platform/persistence/appSettingsStore.ts";
import { DEFAULT_SETTINGS } from "../src/shared/settings/appSettings.ts";

const fixture = () => ({revision: "a".repeat(64), sampled_at_ms: 100, last_heartbeat_ms: 99,
  last_successful_sample_ms: 98, settings: {idle_timeout_secs: 60, timeline_merge_gap_secs: 30,
    min_session_secs: 300, tracking_paused: true, audio_participation_enabled: false,
    web_activity_enabled: true, web_activity_token_present: true, web_activity_port: 12345,
    web_activity_url_privacy: "strip_query"}});

const descriptor = Object.getOwnPropertyDescriptor(globalThis, "window");
Object.defineProperty(globalThis, "window", {configurable: true, value: {}});
try {
  const saved = {...DEFAULT_SETTINGS};
  const edited = {...saved, minSessionSecs: 360};
  const remote = {...saved, idleTimeoutSecs: 1200, minSessionSecs: 420};
  const rebased = rebaseSettingsDraft(saved, edited, remote);
  assert.equal(hasSettingsDraftPolicyConflict(saved, edited, remote), true);
  assert.equal(hasSettingsDraftPolicyConflict(saved, edited, {...saved, idleTimeoutSecs: 1200}), false);
  assert.equal(hasSettingsDraftPolicyConflict(saved, edited, {...edited}), false);
  assert.equal(hasSettingsDraftPolicyEdits(saved, edited), true);
  assert.equal(hasSettingsDraftPolicyEdits(remote, {...remote, themeMode: "dark"}), false);
  assert.equal(rebased.idleTimeoutSecs, 1200);
  assert.equal(rebased.minSessionSecs, 360);
  assert.deepEqual(rebaseSettingsDraft(saved, saved, remote), remote);
  assert.deepEqual(rebaseSettingsDraft(null, null, remote), remote);
  const snapshot = parseProductSettingsSnapshot(fixture());
  assert.equal(snapshot.settings.idleTimeoutSecs, 60);
  assert.equal(snapshot.settings.timelineMergeGapSecs, 30);
  for (const invalid of [null, {}, {...fixture(), revision: "bad"}, {...fixture(), last_heartbeat_ms: -1},
    {...fixture(), settings: {...fixture().settings, idle_timeout_secs: Number.MAX_SAFE_INTEGER + 1}},
    {...fixture(), settings: {...fixture().settings, web_activity_token_present: false}},
    {...fixture(), settings: {...fixture().settings, min_session_secs: 331}},
    {...fixture(), settings: {...fixture().settings, tracking_paused: "1"}},
    {...fixture(), padding: "x".repeat(8192)},
  ]) assert.throws(() => parseProductSettingsSnapshot(invalid));

  const queries: string[] = [];
  mockIPC((command, args) => {
    if (command === "cmd_get_product_settings") return fixture();
    if (command === "plugin:sql|select") {
      const query = String((args as {query: string}).query);
      queries.push(query);
      assert.match(query, /WHERE key IN/);
      const values = (args as {values: string[]}).values;
      assert(!values.includes("tracking_paused"));
      assert(!values.includes("idle_timeout_secs"));
      return [{key: "theme_mode", value: "dark"}, {key: "refresh_interval_secs", value: "3"}];
    }
    throw new Error(`unexpected command ${command}`);
  });
  const settings = await loadAppSettings();
  assert.equal(settings.themeMode, "dark");
  assert.equal(settings.refreshIntervalSecs, 3);
  assert.equal(settings.idleTimeoutSecs, 60); // Do not clamp confirmed policy to the slider range.
  assert.equal(settings.trackingPaused, true);
  assert.equal(queries.length, 1);
  queries.length = 0;
  assert.equal(await loadTrackerHealthTimestamp(), 98);
  assert.equal(queries.length, 0);

  mockIPC((command) => { assert.equal(command, "cmd_get_product_settings"); throw new Error("owner unavailable"); });
  await assert.rejects(loadProductSettingsSnapshot, /owner unavailable/);
  await assert.rejects(loadTrackerHealthTimestamp, /owner unavailable/);
  const mutations: string[] = [];
  mockIPC((command, args) => {
    mutations.push(command);
    assert.equal(command, "cmd_commit_settings_if_revision");
    assert.equal((args as {expectedRevision: string}).expectedRevision, "a".repeat(64));
    throw new Error("product-settings-conflict");
  });
  await assert.rejects(() => saveAppSettingsPatch({minSessionSecs: 360, themeMode: "dark"}, "a".repeat(64)), /product-settings-conflict/);
  assert.deepEqual(mutations, ["cmd_commit_settings_if_revision"]);
  console.log("PASS product settings validation, owner policy, scoped local preferences and no health SQL fallback");
} finally {
  clearMocks();
  if (descriptor) Object.defineProperty(globalThis, "window", descriptor);
  else Reflect.deleteProperty(globalThis, "window");
}
