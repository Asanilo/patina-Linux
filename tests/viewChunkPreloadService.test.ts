import assert from "node:assert/strict";
import {
  scheduleLazyViewChunkPreload,
  type PreloadableView,
} from "../src/app/services/viewChunkPreloadService.ts";

type ScheduledTask = {
  callback: () => void;
  cancelled: boolean;
  delayMs: number;
  idleTimeoutMs: number;
};

function createTaskScheduler() {
  const tasks: ScheduledTask[] = [];
  return {
    tasks,
    schedule(callback: () => void, delayMs: number, idleTimeoutMs: number) {
      const task = {
        callback,
        cancelled: false,
        delayMs,
        idleTimeoutMs,
      };
      tasks.push(task);
      return () => {
        task.cancelled = true;
      };
    },
    runNext() {
      const task = tasks.shift();
      if (!task || task.cancelled) {
        return task;
      }

      task.callback();
      return task;
    },
  };
}

function createLoaders(
  calls: PreloadableView[],
  failingView?: PreloadableView,
) {
  const buildLoader = (view: PreloadableView) => async () => {
    calls.push(view);
    if (view === failingView) {
      throw new Error(`${view} failed`);
    }
  };

  return {
    history: buildLoader("history"),
    settings: buildLoader("settings"),
    mapping: buildLoader("mapping"),
    data: buildLoader("data"),
  };
}

async function flushPromises() {
  await Promise.resolve();
  await Promise.resolve();
}

let passed = 0;
async function runTest(name: string, fn: () => void | Promise<void>) {
  await fn();
  passed += 1;
  console.log(`PASS ${name}`);
}

await runTest("preloads configured chunks sequentially", async () => {
  const scheduler = createTaskScheduler();
  const calls: PreloadableView[] = [];

  scheduleLazyViewChunkPreload({
    views: ["history", "settings", "mapping"],
    initialDelayMs: 12,
    staggerMs: 3,
    idleTimeoutMs: 8,
  }, {
    loaders: createLoaders(calls),
    schedule: scheduler.schedule,
    warn: () => {
      throw new Error("unexpected warning");
    },
  });

  assert.equal(scheduler.tasks.length, 1);
  assert.equal(scheduler.tasks[0].delayMs, 12);
  assert.equal(scheduler.tasks[0].idleTimeoutMs, 8);

  scheduler.runNext();
  await flushPromises();
  assert.deepEqual(calls, ["history"]);
  assert.equal(scheduler.tasks[0].delayMs, 3);

  scheduler.runNext();
  await flushPromises();
  assert.deepEqual(calls, ["history", "settings"]);

  scheduler.runNext();
  await flushPromises();
  assert.deepEqual(calls, ["history", "settings", "mapping"]);
  assert.equal(scheduler.tasks.length, 0);
});

await runTest("keeps preloading after a chunk failure and reports the warning", async () => {
  const scheduler = createTaskScheduler();
  const calls: PreloadableView[] = [];
  const warnings: Array<{ message: string; error: unknown }> = [];

  scheduleLazyViewChunkPreload({
    views: ["history", "settings", "data"],
    initialDelayMs: 0,
    staggerMs: 0,
  }, {
    loaders: createLoaders(calls, "settings"),
    schedule: scheduler.schedule,
    warn: (message, error) => warnings.push({ message, error }),
  });

  scheduler.runNext();
  await flushPromises();
  scheduler.runNext();
  await flushPromises();
  scheduler.runNext();
  await flushPromises();

  assert.deepEqual(calls, ["history", "settings", "data"]);
  assert.equal(warnings.length, 1);
  assert.match(warnings[0].message, /settings/);
  assert.ok(warnings[0].error instanceof Error);
});

await runTest("does not run queued tasks after cancellation", async () => {
  const scheduler = createTaskScheduler();
  const calls: PreloadableView[] = [];

  const cancel = scheduleLazyViewChunkPreload({
    views: ["history", "settings"],
    initialDelayMs: 0,
    staggerMs: 0,
  }, {
    loaders: createLoaders(calls),
    schedule: scheduler.schedule,
    warn: () => {
      throw new Error("unexpected warning");
    },
  });

  cancel();
  const cancelledTask = scheduler.runNext();
  await flushPromises();

  assert.equal(cancelledTask?.cancelled, true);
  assert.deepEqual(calls, []);
  assert.equal(scheduler.tasks.length, 0);
});

await runTest("cancels later chunks after the current preload settles", async () => {
  const scheduler = createTaskScheduler();
  const calls: PreloadableView[] = [];

  const cancel = scheduleLazyViewChunkPreload({
    views: ["history", "settings"],
    initialDelayMs: 0,
    staggerMs: 0,
  }, {
    loaders: createLoaders(calls),
    schedule: scheduler.schedule,
    warn: () => {
      throw new Error("unexpected warning");
    },
  });

  scheduler.runNext();
  await flushPromises();
  assert.deepEqual(calls, ["history"]);
  assert.equal(scheduler.tasks.length, 1);

  cancel();
  const cancelledTask = scheduler.runNext();
  await flushPromises();

  assert.equal(cancelledTask?.cancelled, true);
  assert.deepEqual(calls, ["history"]);
});

await runTest("defaults preload the core lazy view chunks", async () => {
  const scheduler = createTaskScheduler();
  const calls: PreloadableView[] = [];

  scheduleLazyViewChunkPreload({}, {
    loaders: createLoaders(calls),
    schedule: scheduler.schedule,
    warn: () => {
      throw new Error("unexpected warning");
    },
  });

  assert.equal(scheduler.tasks[0].delayMs, 1200);
  assert.equal(scheduler.tasks[0].idleTimeoutMs, 1500);

  for (let index = 0; index < 4; index += 1) {
    scheduler.runNext();
    await flushPromises();
  }

  assert.deepEqual(calls, ["history", "data", "settings", "mapping"]);
  assert.equal(scheduler.tasks.length, 0);
});

console.log(`Passed ${passed} view chunk preload tests`);
