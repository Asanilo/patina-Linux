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
if (args.some(arg => !["--background-delay", "--autostart-only", "--x11"].includes(arg))
    || new Set(args).size !== args.length
    || (args.includes("--background-delay") && args.includes("--autostart-only"))
    || (args.includes("--x11") && !args.includes("--autostart-only"))) {
  throw new Error("Usage: node scripts/native-window-lifecycle.mjs [--background-delay | --autostart-only [--x11]]");
}
const delayTest = args.includes("--background-delay");
const autostartOnly = args.includes("--autostart-only");
const x11 = args.includes("--x11");
const testName = autostartOnly ? "native_widget_autostart" : delayTest ? "native_background_delay" : "native_window_lifecycle";
if (process.platform !== "linux" || (!x11 && (!process.env.WAYLAND_DISPLAY || !process.env.XDG_RUNTIME_DIR))) {
  throw new Error("Run from a Linux Wayland session; no X11 fallback is used.");
}
const socket = x11 ? undefined : path.resolve(process.env.XDG_RUNTIME_DIR, process.env.WAYLAND_DISPLAY);
if (socket && !(await stat(socket)).isSocket()) throw new Error("Wayland display is not a socket");
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
  ...(x11 ? { GDK_BACKEND: "x11", XDG_SESSION_TYPE: "x11", GDK_SYNCHRONIZE: "1", WEBKIT_DISABLE_COMPOSITING_MODE: "1" }
    : { WAYLAND_DISPLAY: socket, GDK_BACKEND: "wayland", XDG_SESSION_TYPE: "wayland" }),
  PATINA_NATIVE_TEST_ROOT: root,
  PATINA_NATIVE_TEST_URL: `http://127.0.0.1:${server.address().port}`,
};
console.log(`Native evidence: ${root} (${autostartOnly ? "20 creation cycles" : delayTest ? "about five minutes" : "about eleven minutes"}; no production runtime)`);
const log = createWriteStream(path.join(root, "native.log"), { flags: "wx", mode: 0o600 });
const testArgs = ["--", binary, `app::native_window_tests::${testName}`, "--exact", "--ignored", "--nocapture", "--test-threads=1"];
const child = spawn(x11 ? "xvfb-run" : "dbus-run-session", x11
  ? ["-a", "-s", "-screen 0 1280x720x24 -noreset", "dbus-run-session", ...testArgs]
  : testArgs, {
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
