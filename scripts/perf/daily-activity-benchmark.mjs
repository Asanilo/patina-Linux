// Explicit opt-in query-worker evidence, never production profile discovery.
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { mkdtemp, writeFile, readFile, chmod } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const observed = process.argv.includes("--observed-apps");
const trend = process.argv.includes("--trend");
const apps = process.argv.includes("--daily-apps");
if ([trend, observed, apps].filter(Boolean).length > 1) throw new Error("Choose one benchmark mode");
const modes = apps ? ["apps-legacy", "apps"] : trend ? ["trend-legacy", "trend"] : observed ? ["observed-legacy", "observed"] : ["legacy", "daily"];
if (process.platform !== "linux") throw new Error("This benchmark requires Linux /proc");
async function sha256(file) {
  const hasher = createHash("sha256");
  for await (const chunk of createReadStream(file)) hasher.update(chunk);
  return hasher.digest("hex");
}
const root = await mkdtemp("/tmp/patina-daily-bench-");
await chmod(root, 0o700);
await writeFile(path.join(root, "marker"), "daily-query-benchmark\n", { flag: "wx", mode: 0o600 });
console.log(`Query benchmark evidence: ${root}`);
let binary;
const build = spawn("cargo", ["test", "--manifest-path", "src-tauri/Cargo.toml", "--lib", "--no-run", "--message-format=json"], {
  cwd: repo, stdio: ["ignore", "pipe", "inherit"],
});
const built = new Promise((resolve, reject) => { build.once("error", reject); build.once("exit", resolve); });
for await (const line of createInterface({ input: build.stdout })) {
  let event;
  try { event = JSON.parse(line); } catch { continue; }
  if (event.reason === "compiler-artifact" && event.target.name === "patina_lib" && event.profile.test) binary = event.executable;
  if (event.reason === "compiler-message" && event.message.rendered) process.stderr.write(event.message.rendered);
}
if (await built !== 0 || !binary) throw new Error("Could not compile benchmark");
const hash = await sha256(binary);
let fixtureHash;
for (const mode of ["seed", ...modes]) {
  const child = spawn(binary, ["data::repositories::daily_activity::benchmark::query_worker", "--exact", "--ignored", "--nocapture", "--test-threads=1"], {
    cwd: root, stdio: "inherit", env: {
      PATH: process.env.PATH, LANG: "C.UTF-8", TZ: "UTC",
      HOME: root, XDG_DATA_HOME: root, XDG_CONFIG_HOME: root, XDG_CACHE_HOME: root,
      PATINA_DAILY_BENCH_ROOT: root, PATINA_DAILY_BENCH_MODE: mode,
      PATINA_DAILY_BENCH_TREND: trend || apps ? "1" : "0",
    },
  });
  const timer = setTimeout(() => child.kill("SIGKILL"), 120_000);
  const code = await new Promise((resolve, reject) => { child.once("error", reject); child.once("exit", resolve); }).finally(() => clearTimeout(timer));
  if (code !== 0) throw new Error(`${mode} failed; partial evidence kept at ${root}`);
  const currentHash = await sha256(path.join(root, "fixture.db"));
  if (fixtureHash && fixtureHash !== currentHash) throw new Error("Read-only benchmark changed the fixture database");
  fixtureHash = currentHash;
}
const results = [];
if (trend || apps) {
  const legacyDays = JSON.parse(await readFile(path.join(root, `${modes[0]}-days.json`), "utf8"));
  const boundedDays = JSON.parse(await readFile(path.join(root, `${modes[1]}-days.json`), "utf8"));
  if (JSON.stringify(legacyDays) !== JSON.stringify(boundedDays)) throw new Error("Daily aggregate results differ");
}
for (const mode of modes) {
  const result = JSON.parse(await readFile(path.join(root, `${mode}.json`), "utf8"));
  const peak = {};
  for (const field of ["rss_bytes", "pss_bytes", "uss_bytes"]) {
    const values = result.samples.map(sample => sample.memory[field]);
    peak[field] = values.length && values.every(value => value !== null) ? Math.max(...values) : null;
  }
  results.push({ mode, elapsed_ms: result.elapsed_ms, result: result.result, baseline: result.baseline, sampled_peak: peak, after: result.after });
}
const daily = results.find(result => result.mode === modes[1]);
const budgets = { elapsed_ms: 5000, response_bytes: 64 * 1024, sampled_uss_growth_bytes: 64 * 1024 * 1024 };
const passed = daily.elapsed_ms <= budgets.elapsed_ms
  && daily.result.Ok?.[0] <= budgets.response_bytes
  && daily.sampled_peak.uss_bytes !== null && daily.baseline.uss_bytes !== null
  && daily.sampled_peak.uss_bytes - daily.baseline.uss_bytes <= budgets.sampled_uss_growth_bytes;
const report = { binary, sha256: hash, fixture_sha256: fixtureHash, scope: "Debug query-worker comparison, not Desktop/WebKit/daemon end-to-end acceptance", ...(trend ? { daily_totals_and_top_apps_equal: true } : {}), ...(apps ? { daily_application_totals_equal: true } : {}), budgets, passed, results };
await writeFile(path.join(root, "summary.json"), JSON.stringify(report, null, 2), { flag: "wx", mode: 0o600 });
console.log(JSON.stringify(report, null, 2));
if (!passed) process.exitCode = 1;
