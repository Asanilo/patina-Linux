import assert from "node:assert/strict";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { getDashboardProduct } from "../src/platform/persistence/dashboardRepository.ts";
import { buildDashboardReadModel } from "../src/features/dashboard/services/dashboardReadModel.ts";
import { ProcessMapper } from "../src/shared/classification/processMapper.ts";
import { SnapshotReadController } from "../src/shared/lib/snapshotReadController.ts";
import { dashboardWireFixture } from "./helpers/dashboardProductFixture.ts";
import { buildDashboardReadModel as legacyDashboard } from "./helpers/legacyDashboardReadModel.ts";
import { resolveTrackerHealth } from "../src/shared/types/tracking.ts";

type Fixture = ReturnType<typeof dashboardWireFixture>;
const date = new Date(2026,3,18);
const wire = dashboardWireFixture(date);
const product = await getDashboardProduct(date,async key=>{assert.equal(key,"2026-04-18");return wire;});
ProcessMapper.setUserOverrides({editor:{category:"development",displayName:"Research"}});
const model = buildDashboardReadModel(product);
const start = new Date(2026,3,18,8).getTime();
const legacy = legacyDashboard([{exeName:"editor",appName:"Editor",startTime:start,endTime:start+120000}],resolveTrackerHealth(start+120000,start+120000,8000),start+120000);
assert.equal(model.totalTrackedTime,legacy.totalTrackedTime);
assert.deepEqual(model.hourlyActivity,legacy.hourlyActivity);
assert.deepEqual(model.categoryDist,legacy.categoryDist);
assert.equal(model.topApplications[0].name,legacy.topApplications[0].name);
ProcessMapper.setUserOverrides({editor:{category:"music",displayName:"New name",track:false}});
assert.deepEqual(buildDashboardReadModel(product),model); // No client reclassification or live extrapolation.
ProcessMapper.clearUserOverrides();
for (const mutate of [
  (value:Fixture)=>{value.current.start_ms++;},
  (value:Fixture)=>{value.current.active_ms++;},
  (value:Fixture)=>{value.hours.pop();},
  (value:Fixture)=>{value.hours[1].hour=0;},
  (value:Fixture)=>{value.hours[8].categories[0].category="music";},
  (value:Fixture)=>{value.tracking_health.live_cutoff_ms=1;},
  (value:Fixture)=>{value.hours[8].categories.push(value.hours[8].categories[0]);},
]) {
  const invalid=structuredClone(wire);mutate(invalid);
  await assert.rejects(getDashboardProduct(date,async()=>invalid));
}
const descriptor = Object.getOwnPropertyDescriptor(globalThis,"window");
Object.defineProperty(globalThis,"window",{configurable:true,value:{}});
try {
  const calls:string[]=[];
  mockIPC(command=>{calls.push(command);if(command!=="cmd_get_dashboard_product")throw new Error("unexpected SQL or command");return wire;});
  assert.equal((await getDashboardProduct(date)).current.duration,120000);
  assert.deepEqual(calls,["cmd_get_dashboard_product"]);
  mockIPC(()=>{throw new Error("backend unavailable");});
  await assert.rejects(getDashboardProduct(date),/backend unavailable/);
} finally {clearMocks();if(descriptor)Object.defineProperty(globalThis,"window",descriptor);else Reflect.deleteProperty(globalThis,"window");}

const turn=()=>new Promise(resolve=>setTimeout(resolve,0));
let reads=0, cache=0;
const pending:Array<{resolve:(value:number)=>void;reject:(error:unknown)=>void}>=[];
const output:number[]=[],errors:unknown[]=[];
const owner=new SnapshotReadController(()=>{reads++;return new Promise<number>((resolve,reject)=>pending.push({resolve,reject}));},value=>output.push(value),error=>errors.push(error),()=>cache);
owner.refresh();await turn();owner.refresh();owner.refresh();assert.equal(reads,1);
pending[0].resolve(1);await turn();assert.deepEqual(output,[1]);
owner.refresh();await turn();owner.refresh(true);pending[1].resolve(2);await turn();
assert.deepEqual(output,[1]);assert.equal(reads,3);
pending[2].resolve(3);await turn();assert.deepEqual(output,[1,3]);
owner.refresh();await turn();cache++;pending[3].resolve(4);await turn();assert.equal(reads,5);assert.deepEqual(output,[1,3]);
pending[4].reject(new Error("disconnected"));await turn();assert.equal(errors.length,1);assert.deepEqual(output,[1,3]);
owner.refresh();await turn();pending[5].resolve(6);await turn();assert.deepEqual(output,[1,3,6]);
owner.refresh();await turn();owner.dispose();pending[6].resolve(7);await turn();owner.refresh();assert.equal(reads,7);assert.deepEqual(output,[1,3,6]);
console.log("Dashboard owner-only transport, presentation parity, snapshot validation and refresh lifecycle passed");

let disposedReads = 0;
const immediatelyDisposed = new SnapshotReadController(async () => ++disposedReads, () => assert.fail("disposed publication"), () => assert.fail("disposed error"), () => 0);
immediatelyDisposed.refresh(); immediatelyDisposed.dispose(); await turn();
assert.equal(disposedReads, 0, "disposal before the scheduled read must not enqueue a backend request");
