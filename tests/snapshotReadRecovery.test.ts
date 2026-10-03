import assert from "node:assert/strict";
import { SnapshotReadController } from "../src/shared/lib/snapshotReadController.ts";

const turn = () => new Promise(resolve => setTimeout(resolve, 0));
function clock() {
  const jobs: Array<{callback: () => void; delay: number; cancelled: boolean}> = [];
  return {
    jobs,
    schedule(callback: () => void, delay: number) {
      const job = {callback, delay, cancelled: false};
      jobs.push(job);
      return () => {job.cancelled = true;};
    },
  };
}

{
  const timer = clock();
  let attempts = 0;
  const values: number[] = [];
  const owner = new SnapshotReadController(async () => {
    attempts++;
    if (attempts < 3) throw new Error("temporary outage");
    return attempts;
  }, value => values.push(value), () => {}, () => 0,
  {retryDelaysMs: [10, 20], scheduleRetry: timer.schedule});
  owner.refresh(); await turn();
  assert.equal(attempts, 1);
  assert.equal(timer.jobs[0].delay, 10);
  timer.jobs[0].callback(); await turn();
  assert.equal(attempts, 2);
  assert.equal(timer.jobs[1].delay, 20);
  timer.jobs[1].callback(); await turn();
  assert.deepEqual(values, [3]);
  assert.equal(timer.jobs.length, 2);
  owner.dispose();
}

{
  const timer = clock();
  let attempts = 0;
  const owner = new SnapshotReadController(async () => { attempts++; throw new Error("still unavailable"); },
    () => assert.fail("failed read published"), () => {}, () => 0,
    {retryDelaysMs: [10, 20], scheduleRetry: timer.schedule});
  owner.refresh(); await turn();
  timer.jobs[0].callback(); await turn();
  timer.jobs[1].callback(); await turn();
  assert.equal(attempts, 3);
  assert.equal(timer.jobs.length, 2, "retry exhaustion must not spin");
  owner.refresh(true); await turn();
  assert.equal(timer.jobs[2].delay, 10, "new invalidation starts a new bounded attempt sequence");
  owner.dispose();
  assert.equal(timer.jobs[2].cancelled, true);
  timer.jobs[2].callback(); await turn();
  assert.equal(attempts, 4, "a late timer must not read after disposal");
}

{
  const timer = clock();
  let attempts = 0;
  let fail = true;
  const values: number[] = [];
  const owner = new SnapshotReadController(async () => { attempts++; if (fail) throw new Error("temporary"); return attempts; },
    value => values.push(value), () => {}, () => 0,
    {retryDelaysMs: [10, 20], scheduleRetry: timer.schedule});
  owner.refresh(); await turn();
  fail = false;
  owner.refresh(true); await turn();
  assert.equal(timer.jobs[0].cancelled, true);
  assert.deepEqual(values, [2]);
  timer.jobs[0].callback(); await turn();
  assert.equal(attempts, 2, "a cancelled old timer cannot start an extra read after success");
  fail = true;
  owner.refresh(); await turn();
  assert.equal(timer.jobs[1].delay, 10, "success resets the failure backoff");
  owner.dispose();
}

{
  const timer = clock();
  let scope = "old";
  let attempts = 0;
  const values: string[] = [];
  const owner = new SnapshotReadController(async () => {attempts++; if (scope === "old") throw new Error("old scope unavailable"); return scope;},
    value => values.push(value), () => {}, () => scope,
    {retryDelaysMs: [10], scheduleRetry: timer.schedule});
  owner.refresh(); await turn();
  scope = "new";
  timer.jobs[0].callback(); await turn();
  assert.equal(attempts, 2);
  assert.deepEqual(values, ["new"]);
  owner.dispose();
}

console.log("PASS snapshot read recovery: bounded retries, reset, invalidation, scope changes and disposal");

{
  const timer = clock();
  let scope = 1;
  const owner = new SnapshotReadController(async () => {throw new Error("unavailable");},
    () => assert.fail("failed scope published"), () => {}, () => scope,
    {retryDelaysMs: [10, 20], scheduleRetry: timer.schedule});
  owner.refresh(); await turn();
  timer.jobs[0].callback(); await turn();
  assert.equal(timer.jobs[1].delay, 20);
  scope = 2;
  owner.refresh(); await turn();
  assert.equal(timer.jobs[1].cancelled, true);
  assert.equal(timer.jobs[2].delay, 10, "an explicit read in a new scope receives its own retry budget");
  owner.dispose();
}
