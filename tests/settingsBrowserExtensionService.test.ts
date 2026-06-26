import assert from "node:assert/strict";
import {
  buildBrowserExtensionConfigText,
} from "../src/features/settings/services/settingsBrowserExtensionService.ts";

let passed = 0;

async function runTest(name: string, fn: () => Promise<void> | void) {
  await fn();
  passed += 1;
  console.log(`PASS ${name}`);
}

await runTest("browser extension config copy text includes local bridge base url and token", () => {
  assert.equal(
    buildBrowserExtensionConfigText({ port: 12345, token: "secret-token" }),
    [
      "Patina Web Activity",
      "Port: 12345",
      "Token: secret-token",
    ].join("\n"),
  );
});

await runTest("browser extension config copy text trims token and keeps the selected port", () => {
  assert.equal(
    buildBrowserExtensionConfigText({ port: 14840, token: "  token-with-spaces  " }),
    [
      "Patina Web Activity",
      "Port: 14840",
      "Token: token-with-spaces",
    ].join("\n"),
  );
});

console.log(`Passed ${passed} settings browser extension service tests`);
