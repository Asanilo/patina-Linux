// Opt-in real Wayland test; compile normally, execute only with private roots/bus.
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { createInterface } from "node:readline";
import { mkdtemp, mkdir, writeFile, stat } from "node:fs/promises";
import { createWriteStream } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
if (args.some(arg => arg !== "--background-delay") || args.length > 1) {
  throw new Error("Usage: node scripts/native-window-lifecycle.mjs [--background-delay]");
}
const delayTest = args.includes("--background-delay");
const testName = delayTest ? "native_background_delay" : "native_window_lifecycle";
if (process.platform !== "linux" || !process.env.WAYLAND_DISPLAY || !process.env.XDG_RUNTIME_DIR) {
  throw new Error("Run from a Linux Wayland session; no X11 fallback is used.");
}
const socket = path.resolve(process.env.XDG_RUNTIME_DIR, process.env.WAYLAND_DISPLAY);
if (!(await stat(socket)).isSocket()) throw new Error("Wayland display is not a socket");
let binary;
const build = spawn("cargo", ["test", "--manifest-path", "src-tauri/Cargo.toml", "--lib", "--no-run", "--message-format=json"], {
  cwd: repo, stdio: ["ignore", "pipe", "inherit"],
});
const buildExit = new Promise((resolve, reject) => {
  build.once("error", reject); build.once("exit", resolve);
});
for await (const line of createInterface({ input: build.stdout })) {
  let event;
  try { event = JSON.parse(line); } catch { continue; }
  if (event.reason === "compiler-artifact" && event.target.name === "patina_lib" && event.profile.test && event.executable) binary = event.executable;
  if (event.reason === "compiler-message" && event.message.rendered) process.stderr.write(event.message.rendered);
}
if (await buildExit !== 0 || !binary) throw new Error("Native test binary was not built");
const root = await mkdtemp("/tmp/patina-window-test-");
for (const dir of ["home", "config", "data", "cache", "runtime"]) await mkdir(path.join(root, dir), { mode: 0o700 });
await writeFile(path.join(root, "marker"), "native-window-lifecycle\n", { mode: 0o600 });
const server = createServer((_req, res) => {
  res.writeHead(200, { "Content-Type": "text/html" });
  res.end("<!doctype html><title>Isolated window lifecycle</title><p>Native lifecycle fixture</p>");
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
const env = {
  PATH: process.env.PATH, LANG: "C.UTF-8", HOME: path.join(root, "home"),
  XDG_CONFIG_HOME: path.join(root, "config"), XDG_DATA_HOME: path.join(root, "data"),
  XDG_CACHE_HOME: path.join(root, "cache"), XDG_RUNTIME_DIR: path.join(root, "runtime"),
  WAYLAND_DISPLAY: socket, GDK_BACKEND: "wayland", XDG_SESSION_TYPE: "wayland",
  PATINA_NATIVE_TEST_ROOT: root,
  PATINA_NATIVE_TEST_URL: `http://127.0.0.1:${server.address().port}`,
};
console.log(`Native evidence: ${root} (${delayTest ? "about five" : "about eleven"} minutes; no production runtime)`);
const log = createWriteStream(path.join(root, "native.log"), { flags: "wx", mode: 0o600 });
const child = spawn("dbus-run-session", ["--", binary, `app::native_window_tests::${testName}`, "--exact", "--ignored", "--nocapture", "--test-threads=1"], {
  cwd: root, env, detached: true, stdio: ["ignore", "pipe", "pipe"],
});
let timedOut = false;
function stop(signal = "SIGTERM") { if (child.pid) { try { process.kill(-child.pid, signal); } catch {} } }
const timer = setTimeout(() => { timedOut = true; stop(); }, 720_000);
const killTimer = setTimeout(() => { if (child.pid) { try { process.kill(-child.pid, "SIGKILL"); } catch {} } }, 725_000);
process.once("SIGINT", stop);
process.once("SIGTERM", stop);
for (const stream of [child.stdout, child.stderr]) stream.on("data", data => { log.write(data); process.stdout.write(data); });
let code;
try {
  code = await new Promise((resolve, reject) => { child.once("error", reject); child.once("exit", resolve); });
} finally {
  clearTimeout(timer); clearTimeout(killTimer); stop();
  child.stdout.destroy(); child.stderr.destroy();
  // Private D-Bus helpers can survive the test executable and ignore SIGTERM.
  await new Promise(resolve => setTimeout(resolve, 1000));
  stop("SIGKILL");
  process.removeListener("SIGINT", stop); process.removeListener("SIGTERM", stop);
  server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
  await new Promise(resolve => log.end(resolve));
}
await writeFile(path.join(root, "result.json"), JSON.stringify({ binary, testName, code, timedOut, passed: code === 0 && !timedOut }, null, 2), { mode: 0o600 });
if (code !== 0 || timedOut) process.exitCode = 1;
