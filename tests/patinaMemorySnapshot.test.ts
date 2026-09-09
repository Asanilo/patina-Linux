import assert from "node:assert/strict";
import { lstat, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { classifyProcesses, parseMemory, processRole, summarizeMemory, writeMemoryEvidence } from "../scripts/patina-memory-snapshot.ts";

assert.deepEqual(parseMemory("Rss: 100 kB\nPss: 60 kB\nPrivate_Clean: 10 kB\nPrivate_Dirty: 20 kB\nPrivate_Hugetlb: 0 kB\nSwap: 0 kB"),
  { rssKiB: 100, pssKiB: 60, ussKiB: 30, swapKiB: 0 });
assert.deepEqual(parseMemory(""), { rssKiB: null, pssKiB: null, ussKiB: null, swapKiB: null });
assert.equal(parseMemory("Pss: invalid kB").pssKiB, null);
assert.equal(processRole("/usr/bin/Patina (deleted)"), "desktop");
assert.equal(processRole("/usr/bin/patinad"), "daemon");
assert.equal(processRole("/tmp/Patina"), null);
const records = [
  { pid: 1, ppid: 0, name: "Patina", role: "desktop" as const, startTime: "1" },
  { pid: 2, ppid: 1, name: "WebKitWebProcess", role: null, startTime: "2" },
  { pid: 3, ppid: 2, name: "child", role: null, startTime: "3" },
  { pid: 4, ppid: 1, name: "patinad", role: "daemon" as const, startTime: "4" },
  { pid: 5, ppid: 0, name: "WebKitWebProcess", role: null, startTime: "5" },
  { pid: 6, ppid: 7, name: "cycle", role: null, startTime: "6" },
  { pid: 7, ppid: 6, name: "cycle", role: null, startTime: "7" },
];
const classified = classifyProcesses(records);
assert.deepEqual(classified.map((row) => [row.pid, row.owner]), [[1, "desktop"], [2, "desktop"], [3, "desktop"], [4, "daemon"]]);
const sums = summarizeMemory([
  { owner: "desktop", ...parseMemory("Rss: 10 kB\nPss: 8 kB") },
  { owner: "desktop", ...parseMemory("") },
  { owner: "daemon", ...parseMemory("Rss: 4 kB\nPss: 3 kB") },
]);
assert.equal(sums[0].pssKiB, null);
assert.equal(sums[0].missingPssProcesses, 1);
assert.equal(sums[1].pssKiB, 3);
const root = await mkdtemp(path.join(tmpdir(), "patina-memory-test-"));
try {
  const file = path.join(root, "evidence.json");
  await writeMemoryEvidence(file, { safe: true });
  assert.deepEqual(JSON.parse(await readFile(file, "utf8")), { safe: true });
  if (process.platform !== "win32") assert.equal((await lstat(file)).mode & 0o777, 0o600);
  await assert.rejects(writeMemoryEvidence(file, { overwrite: true }));
  assert.deepEqual(JSON.parse(await readFile(file, "utf8")), { safe: true });
} finally { await rm(root, { recursive: true, force: true }); }
console.log("PASS memory parsing, owner attribution, missing data and private non-overwriting evidence");
