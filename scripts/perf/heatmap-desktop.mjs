// Explicit opt-in real React/IPC/daemon acceptance; never discover production profiles.
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { createInterface } from 'node:readline';
import { mkdtemp, mkdir, chmod, writeFile, readFile, readdir, stat, realpath, rename } from 'node:fs/promises';
import { createWriteStream, createReadStream } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseMemory, summarizeMemory } from '../patina-memory-snapshot.ts';

const script = fileURLToPath(import.meta.url);
const repo = path.resolve(path.dirname(script), '../..');
const testName = 'app::heatmap_acceptance_tests::heatmap_desktop_worker';
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function hash(file) {
  const digest = createHash('sha256');
  for await (const part of createReadStream(file)) digest.update(part);
  return digest.digest('hex');
}
function launch(command, args, options) {
  const child = spawn(command, args, options);
  child.done = new Promise((resolve, reject) => { child.once('error', reject); child.once('exit', code => resolve(code)); });
  // Keep spawn errors handled even while the supervisor is waiting for readiness.
  child.done.catch(() => {});
  return child;
}
async function writeNew(file, value) {
  await writeFile(file, JSON.stringify(value,null,2), {flag:'wx',mode:0o600});
}
async function capture(owners) {
  const rows = [];
  const seen = new Set();
  async function visit(pid, owner) {
    if (seen.has(pid)) return;
    seen.add(pid);
    try {
      const proc = `/proc/${pid}`;
      const before = await readFile(`${proc}/stat`,'utf8');
      const identity = text => text.slice(text.lastIndexOf(')')+2).split(/\s+/)[19];
      let memory = parseMemory('');
      try {
        memory = parseMemory(await readFile(`${proc}/smaps_rollup`,'utf8'));
        if (identity(await readFile(`${proc}/stat`,'utf8')) !== identity(before)) memory = parseMemory('');
      } catch {}
      rows.push({pid,owner,startTime:identity(before),name:before.slice(before.indexOf('(')+1,before.lastIndexOf(')')),...memory});
      for (const tid of await readdir(`${proc}/task`)) {
        const children = await readFile(`${proc}/task/${tid}/children`,'utf8').catch(() => '');
        for (const child of children.trim().split(/\s+/).filter(Boolean)) await visit(Number(child),owner);
      }
    } catch { rows.push({pid,owner,name:'exited-or-unreadable',...parseMemory('')}); }
  }
  for (const [owner,child] of Object.entries(owners)) if (child?.pid) {
    if (child.exitCode !== null || child.signalCode !== null) {
      rows.push({pid:child.pid,owner,name:'exited',...parseMemory('')});
    } else await visit(child.pid,owner);
  }
  return {at_ms:Date.now(),processes:rows,totals:summarizeMemory(rows)};
}

async function worker(root, binary) {
  if (await realpath(root) !== root || path.dirname(root) !== '/tmp' || !path.basename(root).startsWith('patina-heatmap-test-')
      || await readFile(path.join(root,'marker'),'utf8') !== 'heatmap-acceptance\n') throw new Error('Not an isolated root');
  const env = {...process.env,DBUS_SYSTEM_BUS_ADDRESS:process.env.DBUS_SESSION_BUS_ADDRESS};
  const apiReservation = createServer();
  await new Promise(resolve => apiReservation.listen(0,'127.0.0.1',resolve));
  env.PATINA_HEATMAP_TEST_PORT = String(apiReservation.address().port);
  const run = mode => {
    const log = createWriteStream(path.join(root,`${mode}.log`),{flags:'wx',mode:0o600});
    const child = launch(binary,[testName,'--exact','--ignored','--nocapture','--test-threads=1'],{
      cwd:root,env:{...env,PATINA_HEATMAP_TEST_MODE:mode},stdio:['ignore','pipe','pipe'],
    });
    child.stdout.pipe(log,{end:false}); child.stderr.pipe(log,{end:false});
    child.logDone = new Promise(resolve => child.once('close',() => log.end(resolve)));
    return child;
  };
  let round = 0;
  const events = [];
  const samples = [];
  const owners = {};
  let sampling = true;
  let sampler;
  let passed = false;
  const server = createServer(async (req,res) => {
    try {
      if (req.method === 'POST' && req.url === '/__acceptance') {
        let body = '';
        for await (const chunk of req) { body += chunk; if (body.length > 65536) throw new Error('report too large'); }
        const event = JSON.parse(body);
        if (!['dashboard','heatmap','complete'].includes(event.phase)) throw new Error('invalid phase');
        events.push({at_ms:Date.now(),...event});
        console.log(`UI ${event.phase}${event.passed === undefined ? '' : ` passed=${event.passed}`}`);
        if (event.phase === 'complete') {
          const target = path.join(root,`ui-${++round}.json`);
          await writeNew(`${target}.pending`,event);
          await rename(`${target}.pending`,target);
        }
        res.writeHead(200); res.end('ok'); return;
      }
      if (req.method !== 'GET') {res.writeHead(405);res.end();return;}
      const pathname = decodeURIComponent(new URL(req.url,'http://localhost').pathname);
      const file = path.resolve(repo,'dist',pathname === '/' ? 'index.html' : `.${pathname}`);
      if (!file.startsWith(path.join(repo,'dist')+path.sep)) throw new Error('invalid asset');
      const mime = {'.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.svg':'image/svg+xml'}[path.extname(file)] || 'application/octet-stream';
      res.writeHead(200,{'Content-Type':mime}); res.end(await readFile(file));
    } catch { if (!res.headersSent) res.writeHead(400);res.end(); }
  });
  try {
    await new Promise(resolve => server.listen(0,'127.0.0.1',resolve));
    env.PATINA_HEATMAP_TEST_URL = `http://127.0.0.1:${server.address().port}`;
    const seed = run('seed');
    if (await seed.done !== 0) throw new Error('seed failed');
    await seed.logDone;
    await new Promise(resolve => apiReservation.close(resolve));
    owners.daemon = run('daemon');
    let ready = false;
    for (let i=0;i<150;i++) {
      if (owners.daemon.exitCode !== null) throw new Error('daemon exited during startup');
      try {
        const token = (await readFile(path.join(root,'data/Patina Local/api_token'),'utf8')).trim();
        const response = await fetch(`http://127.0.0.1:${env.PATINA_HEATMAP_TEST_PORT}/api/v1/capabilities`,{headers:{Authorization:`Bearer ${token}`},signal:AbortSignal.timeout(1000)});
        if (response.ok) {ready=true;break;}
      } catch {}
      await delay(200);
    }
    if (!ready) throw new Error('daemon readiness timeout');
    owners.desktop = run('desktop');
    sampler = (async () => { while(sampling) {samples.push(await capture(owners));await delay(250);} })();
    if (await owners.desktop.done !== 0) throw new Error('desktop acceptance failed');
    await owners.desktop.logDone;
    if (round !== 2 || events.some(event => event.phase === 'complete' && !event.passed)) throw new Error('missing UI acceptance');
    owners.daemon.kill('SIGINT');
    if (await owners.daemon.done !== 0) throw new Error('daemon shutdown failed');
    await owners.daemon.logDone;
    const verify = run('verify');
    if (await verify.done !== 0) throw new Error('fixture integrity failed');
    await verify.logDone;
    passed=true;
  } finally {
    sampling=false; if(sampler) await sampler;
    for (const child of Object.values(owners)) if(child.exitCode === null) child.kill('SIGTERM');
    await delay(1000);
    for (const child of Object.values(owners)) if(child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    await Promise.all(Object.values(owners).map(child => child.done.catch(() => null)));
    server.closeAllConnections();await new Promise(resolve => server.close(resolve));
    if (apiReservation.listening) await new Promise(resolve => apiReservation.close(resolve));
    await writeNew(path.join(root,'evidence.json'),{passed,scope:'Debug real frontend/IPC/daemon preview, private bus, paused tracking, synthetic native sessions; no release or visual-flicker acceptance',events,samples});
  }
}

async function main() {
  if (process.argv[2] === '--worker') return worker(process.argv[3],process.argv[4]);
  if(process.platform !== 'linux' || !process.env.WAYLAND_DISPLAY || !process.env.XDG_RUNTIME_DIR) throw new Error('Requires a Wayland session');
  const socket=path.resolve(process.env.XDG_RUNTIME_DIR,process.env.WAYLAND_DISPLAY);
  if(!(await stat(socket)).isSocket()) throw new Error('Not a Wayland socket');
  const frontend=launch('npm',['run','build'],{cwd:repo,stdio:'inherit'});
  if(await frontend.done !== 0) throw new Error('frontend build failed');
  const build=launch('cargo',['test','--manifest-path','src-tauri/Cargo.toml','--lib','--no-run','--message-format=json'],{cwd:repo,stdio:['ignore','pipe','inherit']});
  let binary;
  for await (const line of createInterface({input:build.stdout})) {
    let event;try{event=JSON.parse(line);}catch{continue;}
    if(event.reason==='compiler-artifact'&&event.target.name==='patina_lib'&&event.profile.test) binary=event.executable;
    if(event.reason==='compiler-message'&&event.message.rendered) process.stderr.write(event.message.rendered);
  }
  if(await build.done !== 0 || !binary) throw new Error('Rust test build failed');
  const root=await mkdtemp('/tmp/patina-heatmap-test-'); await chmod(root,0o700);
  for(const dir of ['home','config','data','cache','runtime']) await mkdir(path.join(root,dir),{mode:0o700});
  await writeFile(path.join(root,'marker'),'heatmap-acceptance\n',{flag:'wx',mode:0o600});
  await writeNew(path.join(root,'build.json'),{binary,sha256:await hash(binary),frontend:await hash(path.join(repo,'dist/index.html'))});
  console.log(`Real frontend acceptance: ${root}; about six minutes, no production runtime`);
  const env={PATH:process.env.PATH,LANG:'C.UTF-8',TZ:'UTC',HOME:path.join(root,'home'),XDG_CONFIG_HOME:path.join(root,'config'),
    XDG_DATA_HOME:path.join(root,'data'),XDG_CACHE_HOME:path.join(root,'cache'),XDG_RUNTIME_DIR:path.join(root,'runtime'),
    WAYLAND_DISPLAY:socket,GDK_BACKEND:'wayland',XDG_SESSION_TYPE:'wayland',PATINA_HEATMAP_TEST_ROOT:root};
  const child=launch('dbus-run-session',['--',process.execPath,'--experimental-strip-types',script,'--worker',root,binary],{cwd:root,env,detached:true,stdio:'inherit'});
  const stop=(signal='SIGTERM')=>{try{process.kill(-child.pid,signal);}catch{}};
  const interrupt=()=>stop(); process.once('SIGINT',interrupt);process.once('SIGTERM',interrupt);
  let timedOut=false;
  const timer=setTimeout(()=>{timedOut=true;stop();},540000);
  const killTimer=setTimeout(()=>stop('SIGKILL'),545000);
  let code;
  try{code=await child.done;}finally{
    clearTimeout(timer);clearTimeout(killTimer);stop();await delay(1000);stop('SIGKILL');
    process.removeListener('SIGINT',interrupt);process.removeListener('SIGTERM',interrupt);
  }
  await writeNew(path.join(root,'result.json'),{code,timedOut,passed:code===0&&!timedOut});
  if(code!==0||timedOut) process.exitCode=1;
}
main().catch(error=>{console.error(error);process.exitCode=1;});
