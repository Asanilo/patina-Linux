import assert from "node:assert/strict";
import {
  LONG_BACKGROUND_DELAY_MS,
  shouldReturnHomeAfterBackground,
} from "../src/app/services/backgroundReturnHomePolicy.ts";

let passed = 0;

function runTest(name: string, fn: () => void) {
  fn();
  passed += 1;
  console.log(`PASS ${name}`);
}

runTest("Data returns home after a long background interval", () => {
  assert.equal(
    shouldReturnHomeAfterBackground({
      backgroundDurationMs: LONG_BACKGROUND_DELAY_MS,
      currentView: "data",
      hasDirtyDraft: false,
    }),
    true,
  );
});

runTest("Data keeps its view after a short background interval", () => {
  assert.equal(
    shouldReturnHomeAfterBackground({
      backgroundDurationMs: LONG_BACKGROUND_DELAY_MS - 1,
      currentView: "data",
      hasDirtyDraft: false,
    }),
    false,
  );
});

runTest("History returns home after a long background interval", () => {
  assert.equal(
    shouldReturnHomeAfterBackground({
      backgroundDurationMs: LONG_BACKGROUND_DELAY_MS,
      currentView: "history",
      hasDirtyDraft: false,
    }),
    true,
  );
});

runTest("non browsing views are not forced home", () => {
  for (const currentView of ["dashboard", "settings", "mapping", "about"] as const) {
    assert.equal(
      shouldReturnHomeAfterBackground({
        backgroundDurationMs: LONG_BACKGROUND_DELAY_MS,
        currentView,
        hasDirtyDraft: false,
      }),
      false,
      `${currentView} should not reset`,
    );
  }
});

runTest("dirty drafts block automatic return home", () => {
  assert.equal(
    shouldReturnHomeAfterBackground({
      backgroundDurationMs: LONG_BACKGROUND_DELAY_MS,
      currentView: "data",
      hasDirtyDraft: true,
    }),
    false,
  );
});

console.log(`Passed ${passed} background return home policy tests`);
