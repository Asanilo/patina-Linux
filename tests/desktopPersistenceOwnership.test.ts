import assert from "node:assert/strict";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { saveAppSettingsPatch } from "../src/platform/persistence/appSettingsStore.ts";
import { commitClassificationSettingMutations } from "../src/platform/persistence/classificationSettingsGateway.ts";
import {
  deleteCustomCategory,
  deleteObservedWebDomainHistory,
  loadDeletedCategories,
  saveAppOverride,
  saveCategoryColorOverride,
  saveCategoryDefaultColorAssignment,
  saveCustomCategory,
  saveDeletedCategory,
} from "../src/features/classification/services/classificationStore.ts";

type Invocation = { command: string; args: Record<string, unknown> | undefined };
const windowDescriptor = Object.getOwnPropertyDescriptor(globalThis, "window");
Object.defineProperty(globalThis, "window", { configurable: true, value: {} });
let passed = 0;

async function runTest(name: string, run: () => Promise<void>) {
  try {
    await run();
    passed += 1;
    console.log(`PASS ${name}`);
  } finally {
    clearMocks();
  }
}

function captureInvocations(handler?: (call: Invocation) => unknown): Invocation[] {
  const calls: Invocation[] = [];
  mockIPC((command, args) => {
    const call = { command, args: args as Record<string, unknown> | undefined };
    calls.push(call);
    return handler?.(call);
  });
  return calls;
}

try {
  await runTest("app settings use one owner command for the whole patch", async () => {
    const calls = captureInvocations();
    await saveAppSettingsPatch({ trackingPaused: true, themeMode: "dark" });
    assert.deepEqual(calls, [{
      command: "cmd_commit_app_settings",
      args: { mutations: [
        { key: "tracking_paused", value: "1" },
        { key: "theme_mode", value: "dark" },
      ] },
    }]);
  });

  for (const command of ["cmd_commit_app_settings", "cmd_commit_classification_settings"]) {
    for (const failure of [
      new Error(`Command ${command} not found`),
      new Error("patinad client is not configured for this profile"),
      new Error("daemon capability unavailable"),
      new Error("database is locked"),
    ]) {
      await runTest(`${command} preserves ${failure.message} without local SQL fallback`, async () => {
        const calls = captureInvocations(({ command: actual }) => {
          if (actual === command) throw failure;
          return undefined;
        });
        const save = command === "cmd_commit_app_settings"
          ? saveAppSettingsPatch({ trackingPaused: true })
          : commitClassificationSettingMutations([{ key: "__deleted_category::music", value: "1" }]);
        await assert.rejects(save, (error: unknown) => error === failure);
        assert.deepEqual(calls.map((call) => call.command), [command]);
      });
    }
  }

  await runTest("empty mutation batches do not contact any persistence backend", async () => {
    const calls = captureInvocations();
    await saveAppSettingsPatch({});
    await commitClassificationSettingMutations([]);
    assert.deepEqual(calls, []);
  });

  await runTest("automatic category color assignment and clearing use the owner", async () => {
    const calls = captureInvocations();
    await saveCategoryDefaultColorAssignment("music", "aabbcc");
    await saveCategoryDefaultColorAssignment("music", null);
    assert.deepEqual(calls, ["#AABBCC", null].map((value) => ({
      command: "cmd_commit_classification_settings",
      args: { mutations: [{ key: "__category_default_color_assignment::music", value }] },
    })));
  });

  await runTest("classification mutations have no direct SQL write entry points", async () => {
    const calls = captureInvocations();
    const category = "custom:category_focus" as const;
    await saveAppOverride("chrome.exe", { displayName: "Work", enabled: true });
    await saveAppOverride("chrome.exe", null);
    await saveCategoryColorOverride("music", "123456");
    await saveCategoryColorOverride("music", null);
    await saveCustomCategory(category);
    await deleteCustomCategory(category);
    await saveDeletedCategory("music", true);
    await saveDeletedCategory("music", false);
    assert.equal(calls.length, 8);
    assert.ok(calls.every((call) => call.command === "cmd_commit_classification_settings"));
    assert.deepEqual(calls[5].args, { mutations: [
      { key: `__custom_category::${category}`, value: null },
      { key: `__deleted_category::${category}`, value: null },
      { key: `__category_label_override::${category}`, value: null },
      { key: `__category_default_color_assignment::${category}`, value: null },
    ] });
    const deletion = calls[6].args?.mutations as Array<{ key: string; value: string | null }>;
    assert.equal(deletion[0].key, "__deleted_category::music");
    assert.ok(Number(deletion[0].value) > 0);
    assert.deepEqual(deletion[1], { key: "__category_default_color_assignment::music", value: null });
  });

  await runTest("reading deleted categories cleans invalid legacy entries through the owner", async () => {
    const calls = captureInvocations(({ command }) => {
      if (command === "plugin:sql|select") return [
        { key: "__deleted_category::music" },
        { key: "__deleted_category::system" },
        { key: "__deleted_category::custom:category_old" },
      ];
      return undefined;
    });
    assert.deepEqual(await loadDeletedCategories(), ["music"]);
    assert.deepEqual(calls.map((call) => call.command), [
      "plugin:sql|select", "cmd_commit_classification_settings",
    ]);
    assert.deepEqual(calls[1].args, { mutations: [
      { key: "__deleted_category::system", value: null },
      { key: "__deleted_category::custom:category_old", value: null },
    ] });
  });

  await runTest("legacy cleanup propagates unavailable owner instead of writing locally", async () => {
    const failure = new Error("daemon unavailable");
    const calls = captureInvocations(({ command }) => {
      if (command === "plugin:sql|select") return [{ key: "__deleted_category::system" }];
      if (command === "cmd_commit_classification_settings") throw failure;
      return undefined;
    });
    await assert.rejects(loadDeletedCategories(), (error: unknown) => error === failure);
    assert.deepEqual(calls.map((call) => call.command), [
      "plugin:sql|select", "cmd_commit_classification_settings",
    ]);
  });

  await runTest("corrupt unaddressable category keys are ignored without bypassing owner validation", async () => {
    const calls = captureInvocations(({ command }) => {
      if (command === "plugin:sql|select") return [
        { key: "__deleted_category::music" },
        { key: "__deleted_category::" },
        { key: `__deleted_category::${"无".repeat(100)}` },
      ];
      return undefined;
    });
    assert.deepEqual(await loadDeletedCategories(), ["music"]);
    assert.deepEqual(calls.map((call) => call.command), ["plugin:sql|select"]);
  });

  await runTest("website deletion normalizes the domain and calls the owner once", async () => {
    const calls = captureInvocations();
    await deleteObservedWebDomainHistory(" Example.COM. ");
    await deleteObservedWebDomainHistory("  ");
    assert.deepEqual(calls, [{
      command: "cmd_delete_web_activity_segments_by_domain", args: { domain: "example.com" },
    }]);
  });

  await runTest("failed website deletion preserves the owner error without local DELETE", async () => {
    const failure = new Error("web-domain-delete-unsupported");
    const calls = captureInvocations(() => { throw failure; });
    await assert.rejects(deleteObservedWebDomainHistory("example.com"), (error: unknown) => error === failure);
    assert.deepEqual(calls, [{
      command: "cmd_delete_web_activity_segments_by_domain", args: { domain: "example.com" },
    }]);
  });
} finally {
  if (windowDescriptor) Object.defineProperty(globalThis, "window", windowDescriptor);
  else delete (globalThis as { window?: unknown }).window;
}

console.log(`Passed ${passed} Desktop persistence ownership tests`);
