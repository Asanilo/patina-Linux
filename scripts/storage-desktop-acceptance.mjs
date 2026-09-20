// Explicit opt-in: real React/Tauri/WebKit and daemon, synthetic data, private D-Bus.
// The service-manager fixture exercises the production D-Bus client contract;
// it does not establish installed systemd/login/package acceptance.
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { constants, createReadStream, createWriteStream } from 'node:fs';
import { access, chmod, copyFile, link, lstat, mkdir, mkdtemp, open, readFile, readlink, realpath, rename, stat, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(import.meta.url);
const repo = path.resolve(path.dirname(script), '..');
const testName = 'app::storage_acceptance_tests::storage_desktop_worker';
const stages = ['move-data', 'restore-data', 'move-webview', 'cache', 'verify-cache'];
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const realSystemd = process.argv.includes('--systemd') || process.env.PATINA_STORAGE_TEST_SYSTEMD === '1';
const serviceScope = realSystemd ? 'private D-Bus naming adapter and a real temporary systemd user unit' : 'private D-Bus service-manager fixture';
const scope = `Debug real Settings/IPC/WebKit, real UI app.restart, managed storage maintenance and independent patinad on a ${serviceScope}; synthetic data only. Not installed production service, login, package upgrade, release, or visual-flicker acceptance.`;

async function hash(file) {
  const digest = createHash('sha256');
  for await (const chunk of createReadStream(file)) digest.update(chunk);
  return digest.digest('hex');
}
async function writeNew(file, value) {
  await writeFile(file, JSON.stringify(value, null, 2), { flag: 'wx', mode: 0o600 });
}
function launch(command, args, options) {
  const child = spawn(command, args, options);
  child.done = new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', (code, signal) => resolve({ code, signal }));
  });
  child.done.catch(() => {});
  return child;
}
function captureLog(child, log) {
  child.stdout.pipe(log, { end: false }); child.stderr.pipe(log, { end: false });
  child.logDone = new Promise(resolve => {
    let timer;
    let finished = false;
    const finish = () => {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      child.stdout.unpipe(log); child.stderr.unpipe(log);
      log.end(resolve);
    };
    child.once('close', finish);
    child.once('exit', () => {
      // A D-Bus-activated descendant can retain these inherited pipes after
      // the owned child exits. Bound draining and close only our own streams.
      timer = setTimeout(() => { child.stdout.destroy(); child.stderr.destroy(); finish(); }, 1000);
    });
  });
}
const running = child => child && child.exitCode === null && child.signalCode === null;
async function waitFor(test, label, timeout = 30000, child) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    if (child && !running(child)) throw new Error(`${label}: child exited`);
    const value = await test();
    if (value) return value;
    await delay(100);
  }
  throw new Error(`Timed out: ${label}`);
}
async function completed(child, label, timeout) {
  let timer;
  try {
    const result = await Promise.race([
      (async () => { const result = await child.done; if (child.logDone) await child.logDone; return result; })(),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`Timed out: ${label}`)), timeout); }),
    ]);
    if (result.code !== 0) throw new Error(`${label}: exit ${result.code}, signal ${result.signal}`);
  } finally { clearTimeout(timer); }
}
async function stopChild(child) {
  if (!running(child)) return;
  child.kill('SIGTERM');
  await Promise.race([child.done, delay(child.gracefulStopMs ?? 2000)]).catch(() => {});
  if (running(child)) child.kill('SIGKILL');
  await Promise.race([child.done.catch(() => {}), delay(2000)]);
  if (child.logDone) await Promise.race([child.logDone, delay(2000)]);
}
async function readJson(file) {
  try { return JSON.parse(await readFile(file, 'utf8')); }
  catch (error) { if (error.code === 'ENOENT') return null; throw error; }
}
async function identity(pid) {
  try {
    const fields = (await readFile(`/proc/${pid}/stat`, 'utf8')).split(')').at(-1).trim().split(/\s+/);
    return { state: fields[0], group: Number(fields[2]), start_ticks: fields[19] };
  } catch (error) { if (error.code === 'ENOENT') return null; throw error; }
}
async function liveDesktop(boot, binary, root) {
  if (!boot || !Number.isInteger(boot.pid) || boot.pid <= 0 || typeof boot.start_ticks !== 'string') {
    throw new Error('Missing or invalid Desktop process identity');
  }
  const current = await identity(boot.pid);
  if (!current || current.state === 'Z' || current.start_ticks !== boot.start_ticks
      || current.group !== process.pid || await readlink(`/proc/${boot.pid}/exe`) !== binary
      || !(await readFile(`/proc/${boot.pid}/environ`, 'utf8')).split('\0').includes(`PATINA_STORAGE_TEST_ROOT=${root}`)) {
    throw new Error(`Desktop ${boot.pid} does not belong to this private restart chain`);
  }
}
async function waitForDesktopExit(boot) {
  await waitFor(async () => {
    const current = await identity(boot.pid);
    return !current || current.state === 'Z' || current.start_ticks !== boot.start_ticks;
  }, `Desktop ${boot.pid} exited`, 15000);
}
async function systemctl(args, env) {
  const child = launch('/usr/bin/systemctl', ['--user', ...args], { env, stdio: ['ignore', 'pipe', 'pipe'] });
  let stdout = '';
  let stderr = '';
  let timedOut = false;
  let excessiveOutput = false;
  const capture = (key, chunk) => {
    if (key === 'stdout') stdout += chunk.toString('utf8');
    else stderr += chunk.toString('utf8');
    if (stdout.length + stderr.length > 65536) { excessiveOutput = true; child.kill('SIGKILL'); }
  };
  child.stdout.on('data', chunk => capture('stdout', chunk));
  child.stderr.on('data', chunk => capture('stderr', chunk));
  const closed = new Promise(resolve => child.once('close', resolve));
  const timer = setTimeout(() => { timedOut = true; child.kill('SIGKILL'); }, 20000);
  try {
    const result = await child.done;
    await Promise.race([closed, delay(1000)]);
    if (timedOut || excessiveOutput) throw new Error(`systemctl ${args[0]} ${timedOut ? 'timed out' : 'exceeded output bound'}`);
    return { ...result, stdout: stdout.trim(), stderr: stderr.trim() };
  } finally {
    clearTimeout(timer);
    child.stdout.destroy(); child.stderr.destroy();
    await stopChild(child);
  }
}
async function cleanupSystemd(root) {
  // Derive the sole allowed target from our own mkdtemp root. Evidence JSON
  // never supplies a unit name or a host bus address for this cleanup.
  const name = path.basename(root);
  if (path.dirname(root) !== '/tmp' || !/^patina-storage-test-[A-Za-z0-9]{6}$/.test(name)
      || await realpath(root) !== root) throw new Error('Invalid private root for systemd cleanup');
  const metadata = await lstat(root);
  if (!metadata.isDirectory() || metadata.uid !== process.getuid() || (metadata.mode & 0o777) !== 0o700
      || await readFile(path.join(root, 'marker'), 'utf8') !== 'storage-acceptance\n') {
    throw new Error('Unowned private root for systemd cleanup');
  }
  const runtime = `/run/user/${process.getuid()}`;
  const bus = await lstat(path.join(runtime, 'bus'));
  if (!bus.isSocket() || bus.uid !== process.getuid()) throw new Error('Invalid user systemd bus for cleanup');
  const env = { PATH: '/usr/bin:/bin', LANG: 'C.UTF-8', XDG_RUNTIME_DIR: runtime,
    DBUS_SESSION_BUS_ADDRESS: `unix:path=${runtime}/bus` };
  const unit = `${name}.service`;
  const state = async () => {
    const result = await systemctl(['show', unit, '--property=LoadState', '--value'], env);
    if (result.code !== 0 && result.stdout !== 'not-found') throw new Error(`Temporary unit lookup failed: ${result.stderr}`);
    if (!result.stdout) throw new Error('Temporary unit lookup returned no LoadState');
    return result.stdout;
  };
  const before = await state();
  if (before !== 'not-found') {
    const result = await systemctl(['stop', unit], env);
    if (result.code !== 0 && await state() !== 'not-found') throw new Error(`Temporary unit stop failed: ${result.stderr}`);
  }
  await waitFor(async () => await state() === 'not-found', 'temporary systemd unit collected', 20000);
  return { unit, before, after: 'not-found', passed: true };
}
async function waitForServiceCleanup(root, group, serviceBinary) {
  const marker = await readJson(path.join(root, 'service-worker.json'));
  if (!marker) return; // Published before any real unit can be created.
  if (!Number.isInteger(marker.pid) || marker.pid <= 0 || typeof marker.start_ticks !== 'string') {
    throw new Error('Invalid service cleanup process marker');
  }
  await waitFor(async () => {
    const current = await identity(marker.pid);
    if (!current || current.state === 'Z' || current.start_ticks !== marker.start_ticks) return true;
    if (current.group !== group || await readlink(`/proc/${marker.pid}/exe`) !== serviceBinary) {
      throw new Error('Service cleanup worker identity changed');
    }
    return false;
  }, 'service worker graceful cleanup', 20000);
}
function serviceState(value) {
  if (!value) return null;
  if (!Number.isInteger(value.start_count) || !Number.isInteger(value.stop_count)
      || (value.pid !== null && (!Number.isInteger(value.pid) || value.pid <= 0))) {
    throw new Error('Invalid live service fixture state');
  }
  if (value.pid === null) return null;
  return { pid: value.pid, start_count: value.start_count, stop_count: value.stop_count };
}
async function listen(server) {
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => { server.removeListener('error', reject); resolve(); });
  });
  return server.address().port;
}
async function close(server) {
  server.closeAllConnections();
  if (server.listening) await new Promise(resolve => server.close(resolve));
}

async function worker(root, binary, serviceBinary) {
  if (await realpath(root) !== root || path.dirname(root) !== '/tmp'
      || !path.basename(root).startsWith('patina-storage-test-')
      || (await stat(root)).mode % 512 !== 0o700
      || (await stat(root)).uid !== process.getuid()
      || await readFile(path.join(root, 'marker'), 'utf8') !== 'storage-acceptance\n') {
    throw new Error('Not an isolated storage acceptance root');
  }
  for (const [key, directory] of Object.entries({ HOME: 'home', XDG_CONFIG_HOME: 'config', XDG_DATA_HOME: 'data', XDG_CACHE_HOME: 'cache', XDG_RUNTIME_DIR: 'runtime' })) {
    if (process.env[key] !== path.join(root, directory)) throw new Error(`Non-private worker environment: ${key}`);
  }
  const env = { ...process.env };
  const children = [];
  const events = [];
  let activeStage;
  let restartReadyStage;
  let passed = false;
  let failure;
  const run = mode => {
    const label = mode;
    const log = createWriteStream(path.join(root, `${label}.log`), { flags: 'wx', mode: 0o600 });
    const child = launch(mode === 'service' ? serviceBinary : binary,
      [testName, '--exact', '--ignored', '--nocapture', '--test-threads=1'], {
        cwd: root, env: { ...env, PATINA_STORAGE_TEST_MODE: mode },
        stdio: ['ignore', 'pipe', 'pipe'],
      });
    captureLog(child, log);
    if (mode === 'service') child.gracefulStopMs = 20000;
    children.push(child);
    return child;
  };
  const reservation = createServer();
  const server = createServer(async (request, response) => {
    try {
      if (request.method === 'GET' && request.url.startsWith('/__storage_restart_ready?')) {
        const stage = new URL(request.url, 'http://localhost').searchParams.get('stage');
        response.writeHead(200, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
        response.end(JSON.stringify({ ready: restartReadyStage === stage })); return;
      }
      if (request.method === 'POST' && request.url === '/__storage_acceptance') {
        let bytes = 0;
        const parts = [];
        for await (const chunk of request) {
          bytes += chunk.length;
          if (bytes > 65536) throw new Error('Report exceeds limit');
          parts.push(chunk);
        }
        const report = JSON.parse(Buffer.concat(parts).toString('utf8'));
        if (!activeStage || report.stage !== activeStage || typeof report.passed !== 'boolean') throw new Error('Unexpected stage report');
        const target = path.join(root, `stage-${activeStage}.json`);
        if (await readJson(target)) throw new Error('Duplicate stage report');
        await writeNew(`${target}.pending`, report);
        await rename(`${target}.pending`, target);
        console.log(`Storage UI ${report.stage}: ${report.passed ? 'passed' : 'FAILED'}`);
        response.writeHead(200); response.end('ok'); return;
      }
      if (request.method !== 'GET') { response.writeHead(405); response.end(); return; }
      const pathname = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
      const dist = path.join(repo, 'dist');
      const file = path.resolve(dist, pathname === '/' ? 'index.html' : `.${pathname}`);
      if (!file.startsWith(`${dist}${path.sep}`)) throw new Error('Invalid asset path');
      const mime = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png', '.svg': 'image/svg+xml', '.woff2': 'font/woff2' };
      const body = await readFile(file);
      response.writeHead(200, { 'Content-Type': mime[path.extname(file)] || 'application/octet-stream' });
      response.end(body);
    } catch (error) {
      if (!response.headersSent) response.writeHead(error.code === 'ENOENT' ? 404 : 400);
      response.end();
    }
  });
  try {
    env.PATINA_STORAGE_TEST_PORT = String(await listen(reservation));
    env.PATINA_STORAGE_TEST_URL = `http://127.0.0.1:${await listen(server)}`;
    const busLog = createWriteStream(path.join(root, 'dbus.log'), { flags: 'wx', mode: 0o600 });
    const busAddress = `unix:path=${path.join(root, 'runtime/bus')}`;
    env.DBUS_SESSION_BUS_ADDRESS = busAddress;
    env.DBUS_SYSTEM_BUS_ADDRESS = busAddress;
    const bus = launch('dbus-daemon', ['--session', '--nofork', `--address=${busAddress}`], {
      cwd: root, env, stdio: ['ignore', 'pipe', 'pipe'],
    });
    captureLog(bus, busLog);
    children.push(bus);
    await waitFor(async () => {
      try { return (await stat(path.join(root, 'runtime/bus'))).isSocket(); }
      catch (error) { if (error.code === 'ENOENT') return false; throw error; }
    }, 'private D-Bus socket', 10000, bus);
    await completed(run('seed'), 'synthetic seed', 60000);
    await close(reservation);
    const service = run('service');
    await waitFor(() => readJson(path.join(root, 'service-ready.json')), 'service fixture ready', 30000, service);
    const waitForApi = () => waitFor(async () => {
      try {
        const token = (await readFile(path.join(root, 'data/Patina/api_token'), 'utf8')).trim();
        const response = await fetch(`http://127.0.0.1:${env.PATINA_STORAGE_TEST_PORT}/api/v1/capabilities`, {
          headers: { Authorization: `Bearer ${token}` }, signal: AbortSignal.timeout(1000),
        });
        if (!response.ok) return false;
        const { data } = await response.json();
        return data?.runtime_host === 'daemon' && data.tracking?.owned === true
          && data.tracking.ready === true && data.daemon_service?.owned === true;
      } catch { return false; }
    }, 'private daemon API ready', 30000, service);
    await waitForApi();
    let before = await waitFor(async () => serviceState(await readJson(path.join(root, 'service-state.json'))),
      'live service before first Desktop', 5000, service);
    activeStage = stages[0];
    await writeNew(path.join(root, 'continuation.json'), { stage: activeStage, previous_pid: null });
    // app.restart creates successors itself. A regular log fd survives all
    // successors; pipes belonging to the first child must not be closed early.
    const desktopLog = await open(path.join(root, 'desktop-chain.log'), 'wx', 0o600);
    const desktop = launch(binary, [testName, '--exact', '--ignored', '--nocapture', '--test-threads=1'], {
      cwd: root, env: { ...env, PATINA_STORAGE_TEST_MODE: 'desktop' },
      stdio: ['ignore', desktopLog.fd, desktopLog.fd],
    });
    await desktopLog.close();
    children.push(desktop);
    let previousBoot;
    const desktopPids = new Set();
    for (const [index, stage] of stages.entries()) {
      const boot = await waitFor(() => readJson(path.join(root, `boot-${stage}.json`)),
        `Desktop bootstrap ${stage}`, 30000, service);
      if (boot.stage !== stage || boot.previous_pid !== (previousBoot?.pid ?? null)
          || desktopPids.has(boot.pid) || (index === 0 && boot.pid !== desktop.pid)) {
        throw new Error(`Broken Desktop restart chain at ${stage}`);
      }
      desktopPids.add(boot.pid);
      await liveDesktop(boot, binary, root);
      const report = await waitFor(() => readJson(path.join(root, `stage-${stage}.json`)),
        `UI stage ${stage}`, 120000, service);
      if (!report?.passed) throw new Error(`UI stage ${stage} failed: ${report?.error || 'missing report'}`);
      const native = await waitFor(() => readJson(path.join(root, `native-${stage}.json`)),
        `native stage ${stage}`, 30000, service);
      if (native.pid !== boot.pid || native.embedded_owner !== false) throw new Error(`Wrong native owner at ${stage}`);
      if (index < stages.length - 1) await liveDesktop(boot, binary, root);
      if (previousBoot) {
        const restart = await readJson(path.join(root, `restart-${stages[index - 1]}.json`));
        if (restart?.pid !== previousBoot.pid || restart.tauri_restart_exit !== true) {
          throw new Error(`Previous Desktop did not execute Tauri app.restart before ${stage}`);
        }
        await waitForDesktopExit(previousBoot);
      }
      await waitForApi();
      await delay(250); // The fixture publishes an atomic snapshot every 200 ms.
      const expectedRestarts = ['restore-data', 'move-webview'].includes(stage) ? 1 : 0;
      const after = await waitFor(async () => {
        const current = serviceState(await readJson(path.join(root, 'service-state.json')));
        return current && current.start_count === before.start_count + expectedRestarts
          && current.stop_count === before.stop_count + expectedRestarts ? current : false;
      }, `service counts after ${stage}`, 5000, service);
      if ((after.pid !== before.pid) !== Boolean(expectedRestarts)) throw new Error(`Wrong daemon lifetime during ${stage}`);
      events.push({ stage, at_ms: Date.now(), desktop: boot, before, after, report: `stage-${stage}.json`,
        entered_via: index === 0 ? 'runner initial launch' : 'real UI app.restart' });
      before = after;
      previousBoot = boot;
      // Only open the page's restart gate after evidence and daemon assertions.
      // The next process is created by Tauri, never by the Node supervisor.
      activeStage = stages[index + 1];
      restartReadyStage = stage;
    }
    const completion = await waitFor(() => readJson(path.join(root, 'desktop-completed.json')),
      'last Desktop completed normally', 15000, service);
    if (!completion.passed || completion.pid !== previousBoot.pid) throw new Error('Invalid final Desktop completion');
    await waitForDesktopExit(previousBoot);
    await completed(desktop, 'initial Desktop real restart exit', 5000);
    service.kill('SIGINT');
    await completed(service, 'service fixture shutdown', 20000);
    await completed(run('verify'), 'fixture integrity verification', 60000);
    passed = true;
  } catch (error) {
    failure = String(error);
    throw error;
  } finally {
    for (const child of [...children].reverse()) await stopChild(child);
    await close(server); await close(reservation);
    await writeNew(path.join(root, 'evidence.json'), { passed, scope, failure, events });
  }
}

async function cargoArtifact(args, matches, label) {
  const child = launch('cargo', [...args, '--message-format=json'], { cwd: repo, stdio: ['ignore', 'pipe', 'inherit'] });
  let binary;
  // A bounded build, including compiler stdout collection.
  const timer = setTimeout(() => child.kill('SIGTERM'), 900000);
  const killTimer = setTimeout(() => { child.kill('SIGKILL'); child.stdout.destroy(); }, 905000);
  try {
    for await (const line of createInterface({ input: child.stdout })) {
      let event;
      try { event = JSON.parse(line); } catch { continue; }
      if (event.reason === 'compiler-artifact' && matches(event)) binary = event.executable;
      if (event.reason === 'compiler-message' && event.message.rendered) process.stderr.write(event.message.rendered);
    }
    await completed(child, label, 10000);
    if (!binary) throw new Error(`${label}: executable absent from Cargo output`);
    return binary;
  } finally { clearTimeout(timer); clearTimeout(killTimer); await stopChild(child); }
}
async function main() {
  if (process.argv[2] === '--worker') return worker(process.argv[3], process.argv[4], process.argv[5]);
  let selectedDaemon;
  const seen = new Set();
  for (let index = 2; index < process.argv.length; index += 1) {
    const option = process.argv[index];
    if (!['--systemd', '--daemon'].includes(option) || seen.has(option)) {
      throw new Error('Usage: node scripts/storage-desktop-acceptance.mjs [--systemd] [--daemon /absolute/candidate/patinad]');
    }
    seen.add(option);
    if (option === '--daemon') {
      const candidate = process.argv[++index];
      if (!candidate || !path.isAbsolute(candidate)) throw new Error('--daemon requires an absolute executable path');
      selectedDaemon = await realpath(candidate);
      const metadata = await lstat(candidate);
      if (!metadata.isFile() || selectedDaemon !== path.resolve(candidate) || !(metadata.mode & 0o111)) {
        throw new Error('--daemon must name a canonical regular executable, without symlinks');
      }
      await access(selectedDaemon, constants.X_OK);
    }
  }
  if (process.platform !== 'linux' || !process.env.WAYLAND_DISPLAY || !process.env.XDG_RUNTIME_DIR) {
    throw new Error('Requires a Linux Wayland session');
  }
  const socket = path.resolve(process.env.XDG_RUNTIME_DIR, process.env.WAYLAND_DISPLAY);
  if (!(await stat(socket)).isSocket()) throw new Error('Not a Wayland socket');
  const frontend = launch('npm', ['run', 'build'], { cwd: repo, stdio: 'inherit' });
  try { await completed(frontend, 'frontend build', 300000); }
  finally { await stopChild(frontend); }
  const binary = await cargoArtifact(['test', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', '--no-run'],
    event => event.target.name === 'patina_lib' && event.profile.test, 'Rust acceptance build');
  const daemon = selectedDaemon ?? await cargoArtifact(['build', '--manifest-path', 'src-tauri/Cargo.toml', '--bin', 'patinad'],
    event => event.target.name === 'patinad' && !event.profile.test, 'independent daemon build');
  const root = await mkdtemp('/tmp/patina-storage-test-');
  await chmod(root, 0o700);
  for (const name of ['home', 'config', 'data', 'cache', 'runtime', 'cancelled', 'moved-data', 'moved-webview']) {
    await mkdir(path.join(root, name), { mode: 0o700 });
  }
  await writeFile(path.join(root, 'marker'), 'storage-acceptance\n', { flag: 'wx', mode: 0o600 });
  const serviceBinary = path.join(root, 'service-worker');
  try { await link(binary, serviceBinary); }
  catch (error) { if (error.code !== 'EXDEV') throw error; await copyFile(binary, serviceBinary); }
  await writeNew(path.join(root, 'build.json'), {
    testBinary: { path: binary, sha256: await hash(binary) },
    daemonBinary: { path: daemon, sha256: await hash(daemon),
      source: selectedDaemon ? 'explicit candidate executable' : 'current checkout cargo build' },
    frontend: { path: path.join(repo, 'dist/index.html'), sha256: await hash(path.join(repo, 'dist/index.html')) },
  });
  const env = {
    PATH: process.env.PATH, LANG: 'C.UTF-8', TZ: 'UTC',
    HOME: path.join(root, 'home'), XDG_CONFIG_HOME: path.join(root, 'config'),
    XDG_DATA_HOME: path.join(root, 'data'), XDG_CACHE_HOME: path.join(root, 'cache'), XDG_RUNTIME_DIR: path.join(root, 'runtime'),
    WAYLAND_DISPLAY: socket, GDK_BACKEND: 'wayland', XDG_SESSION_TYPE: 'wayland',
    PATINA_STORAGE_TEST_ROOT: root, PATINA_STORAGE_TEST_BINARY: daemon,
    ...(realSystemd ? { PATINA_STORAGE_TEST_SYSTEMD: '1' } : {}),
  };
  console.log(`Native storage acceptance: ${root}; ${serviceScope}, no installed production service access`);
  const child = launch(process.execPath, [script, '--worker', root, binary, serviceBinary], {
    cwd: root, env, detached: true, stdio: 'inherit',
  });
  const stop = (signal = 'SIGTERM') => { if (child.pid) { try { process.kill(-child.pid, signal); } catch {} } };
  let interrupted = false;
  let forceTimer;
  const beginStop = () => {
    stop();
    forceTimer ??= setTimeout(() => stop('SIGKILL'), 25000);
  };
  const interrupt = () => { interrupted = true; beginStop(); };
  process.once('SIGINT', interrupt); process.once('SIGTERM', interrupt);
  let timedOut = false;
  const timer = setTimeout(() => { timedOut = true; beginStop(); }, 900000);
  let result;
  const cleanup = { passed: true, systemd: realSystemd, errors: [] };
  try { result = await child.done.catch(error => ({ code: null, signal: null, error: String(error) })); }
  finally {
    clearTimeout(timer); clearTimeout(forceTimer);
    stop();
    try { await waitForServiceCleanup(root, child.pid, serviceBinary); }
    catch (error) { cleanup.passed = false; cleanup.errors.push(String(error)); }
    if (realSystemd) {
      try { cleanup.unit = await cleanupSystemd(root); }
      catch (error) { cleanup.passed = false; cleanup.errors.push(String(error)); }
    }
    // The systemd cgroup is independent of this owned process group. Only kill
    // remaining fixture descendants after its graceful/explicit unit cleanup.
    stop('SIGKILL');
    await writeNew(path.join(root, 'cleanup.json'), cleanup);
    process.removeListener('SIGINT', interrupt); process.removeListener('SIGTERM', interrupt);
  }
  const passed = result.code === 0 && !timedOut && !interrupted && cleanup.passed;
  await writeNew(path.join(root, 'result.json'), { ...result, timedOut, interrupted, passed, scope, cleanup: 'cleanup.json' });
  console.log(`Storage acceptance ${passed ? 'passed' : 'FAILED'}; evidence retained in ${root}`);
  if (!passed) process.exitCode = 1;
}
main().catch(error => { console.error(error); process.exitCode = 1; });
