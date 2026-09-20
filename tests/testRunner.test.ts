import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, sep } from "node:path";
import { spawnSync } from "node:child_process";
import { discoverTests, selectTests } from "../scripts/run-tests.ts";

const root = mkdtempSync(join(tmpdir(), "patina-test-runner-"));
try {
  mkdirSync(join(root, "nested"));
  writeFileSync(join(root, "first.test.ts"), "const value: number = 1;\n");
  writeFileSync(join(root, "nested", "second.test.ts"), "");
  writeFileSync(join(root, "nested", "helper.ts"), "");
  const expected = [join(root, "first.test.ts"), join(root, "nested", "second.test.ts")]
    .map((file) => file.split(sep).join("/"));
  assert.deepEqual(discoverTests(root), expected, "Discover new and nested test files exactly once");
  assert.deepEqual(selectTests(expected), expected, "Run every discovered test by default");
  assert.deepEqual(selectTests(expected, {
    [expected[1]]: { reason: "Needs an isolated desktop", command: "npm run test:desktop" },
  }), [expected[0]]);
  assert.throws(() => selectTests(expected, {
    "missing.test.ts": { reason: "stale", command: "npm test" },
  }), /Stale or non-exact/);
  assert.throws(() => selectTests(expected, {
    [expected[1]]: { reason: "", command: "npm test" },
  }), /reason and explicit command/);
  assert.throws(() => selectTests(expected, {
    [expected[1]]: { reason: "Needs a desktop", command: " " },
  }), /reason and explicit command/);
  assert.throws(() => selectTests([]), /No TypeScript tests found/);
  assert.throws(() => selectTests([expected[0]], {
    [expected[0]]: { reason: "Needs a desktop", command: "npm run test:desktop" },
  }), /No TypeScript tests remain/);

  const marker = join(root, "must-not-run");
  const failure = join(root, "failure.test.ts");
  const later = join(root, "later.test.ts");
  writeFileSync(failure, "process.exitCode = 7;\n");
  writeFileSync(later, `import { writeFileSync } from 'node:fs';\nwriteFileSync(${JSON.stringify(marker)}, 'unexpected');\n`);
  const runnerUrl = new URL("../scripts/run-tests.ts", import.meta.url).href;
  const invokeRunner = (files: string[], childExecutable?: string) => spawnSync(process.execPath, [
    "--experimental-strip-types", "--input-type=module", "--eval",
    `import { runTests } from ${JSON.stringify(runnerUrl)};\n`
      + (childExecutable ? `process.execPath = ${JSON.stringify(childExecutable)};\n` : "")
      + `process.exitCode = runTests(${JSON.stringify(files)});`,
  ], { encoding: "utf8" });
  const passing = invokeRunner([expected[0]]);
  assert.equal(passing.status, 0, passing.stderr);
  const failing = invokeRunner([failure, later]);
  assert.equal(failing.status, 7, "Propagate a failing child process status");
  assert.equal(existsSync(marker), false, "Stop after the first failed test");
  assert.match(failing.stderr, /Test failed:/);
  const unlaunchable = invokeRunner([later], join(root, "missing-node"));
  assert.equal(unlaunchable.status, 1, "A child launch error must fail the gate");
  assert.match(unlaunchable.stderr, /ENOENT/);
  assert.equal(existsSync(marker), false);
} finally {
  rmSync(root, { recursive: true, force: true });
}

console.log("PASS test discovery, explicit exceptions and child failure propagation");
