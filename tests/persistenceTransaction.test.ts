import assert from "node:assert/strict";
import {
  createSerializedJobRunner,
  executeWriteBatchWithExecutor,
  isRecoverableSqliteWriteError,
  type SqlWriteOperation,
} from "../src/platform/persistence/sqliteTransactions.ts";
import { buildClassificationDraftChangePlan } from "../src/features/classification/services/classificationDraftState.ts";
import { buildCommitDraftChangePlanSettingMutations } from "../src/features/classification/services/classificationStore.ts";
import {
  buildAppSettingMutations,
  buildRawAppSettingsPatch,
} from "../src/platform/persistence/appSettingsStore.ts";

class FakeWriteExecutor {
  executedStatements: Array<{ query: string; values?: unknown[] }> = [];
  private mutationCount = 0;
  private readonly failAtMutationNumber: number | null;

  constructor(failAtMutationNumber: number | null = null) {
    this.failAtMutationNumber = failAtMutationNumber;
  }

  async execute(query: string, values?: unknown[]): Promise<void> {
    this.mutationCount += 1;
    if (this.failAtMutationNumber !== null && this.mutationCount === this.failAtMutationNumber) {
      throw new Error(`forced failure at mutation ${this.mutationCount}`);
    }

    this.executedStatements.push({ query, values });
  }
}

let passed = 0;

async function runTest(name: string, fn: () => Promise<void> | void) {
  await fn();
  passed += 1;
  console.log(`PASS ${name}`);
}

function assertNoTransactionControlStatements(executor: FakeWriteExecutor) {
  const transactionControlStatements = new Set(["BEGIN IMMEDIATE", "COMMIT", "ROLLBACK"]);
  assert.equal(
    executor.executedStatements.some((statement) => transactionControlStatements.has(statement.query)),
    false,
  );
}

await runTest("executeWriteBatchWithExecutor executes all operations in order", async () => {
  const executor = new FakeWriteExecutor();
  const operations: SqlWriteOperation[] = [
    { query: "INSERT setting A", values: ["a"] },
    { query: "INSERT setting B", values: ["b"] },
  ];

  await executeWriteBatchWithExecutor(executor, operations);

  assert.deepEqual(executor.executedStatements, operations);
  assertNoTransactionControlStatements(executor);
});

await runTest("executeWriteBatchWithExecutor stops after the first failed operation", async () => {
  const executor = new FakeWriteExecutor(2);
  const operations: SqlWriteOperation[] = [
    { query: "INSERT setting A", values: ["a"] },
    { query: "INSERT setting B", values: ["b"] },
    { query: "INSERT setting C", values: ["c"] },
  ];

  await assert.rejects(
    executeWriteBatchWithExecutor(executor, operations),
    /forced failure at mutation 2/,
  );

  assert.deepEqual(executor.executedStatements, [operations[0]]);
  assertNoTransactionControlStatements(executor);
});

await runTest("classification draft keeps application, color and deletion changes in one batch", () => {
  const mutations = buildCommitDraftChangePlanSettingMutations(buildClassificationDraftChangePlan({
    overrides: {},
    webDomainOverrides: {},
    categoryColorOverrides: {},
    categoryLabelOverrides: {},
    customCategories: [],
    deletedCategories: [],
  }, {
    overrides: {
      "chrome.exe": {
        enabled: true,
        displayName: "Work Browser",
      },
    },
    webDomainOverrides: {},
    categoryColorOverrides: {
      development: "#112233",
    },
    categoryLabelOverrides: {},
    customCategories: [],
    deletedCategories: ["music"],
  }));
  assert.ok(mutations.some((mutation) => mutation.key === "__app_override::chrome.exe"));
  assert.ok(mutations.some((mutation) => mutation.key === "__category_color_override::development"
    && mutation.value === "#112233"));
  assert.ok(mutations.some((mutation) => mutation.key === "__deleted_category::music"
    && mutation.value !== null));
});

await runTest("custom category definitions and labels share one mutation batch", () => {
  const category = "custom:category_focus" as const;
  const mutations = buildCommitDraftChangePlanSettingMutations(buildClassificationDraftChangePlan({
    overrides: {},
    webDomainOverrides: {},
    categoryColorOverrides: {},
    categoryLabelOverrides: {},
    customCategories: [],
    deletedCategories: [],
  }, {
    overrides: {},
    webDomainOverrides: {},
    categoryColorOverrides: {},
    categoryLabelOverrides: { [category]: "Deep Focus" },
    customCategories: [category],
    deletedCategories: [],
  }));

  assert.equal(
    mutations.some((mutation) => (
      mutation.key === `__custom_category::${category}` && mutation.value !== null
    )),
    true,
  );
  assert.equal(
    mutations.some((mutation) => (
      mutation.key === `__category_label_override::${category}`
      && mutation.value === "Deep Focus"
    )),
    true,
  );
});

await runTest("settings raw patch persists theme mode with snake case key", () => {
  assert.deepEqual(buildRawAppSettingsPatch({
    backgroundOptimization: true,
    themeMode: "system",
    language: "en-US",
    hourlyActivityChartMode: "category",
    colorSchemeLight: "rose-pine",
    colorSchemeDark: "gruvbox",
  }), {
    background_optimization: true,
    theme_mode: "system",
    language: "en-US",
    hourly_activity_chart_mode: "category",
    color_scheme_light: "rose-pine",
    color_scheme_dark: "gruvbox",
  });
});

await runTest("app setting mutations serialize values for backend transaction command", () => {
  assert.deepEqual(buildAppSettingMutations(buildRawAppSettingsPatch({
    trackingPaused: true,
    timelineMergeGapSecs: 180,
    backgroundOptimization: true,
    backgroundOptimizationDelayMinutes: 1,
    themeMode: "system",
    language: "en-US",
    hourlyActivityChartMode: "category",
  })), [
    { key: "tracking_paused", value: "1" },
    { key: "timeline_merge_gap_secs", value: "180" },
    { key: "background_optimization", value: "1" },
    { key: "background_optimization_delay_minutes", value: "1" },
    { key: "theme_mode", value: "system" },
    { key: "language", value: "en-US" },
    { key: "hourly_activity_chart_mode", value: "category" },
  ]);
});

await runTest("SQLite transient write errors are recoverable", () => {
  assert.equal(isRecoverableSqliteWriteError("database is locked"), true);
  assert.equal(isRecoverableSqliteWriteError(new Error("SQLITE_BUSY: database is busy")), true);
  assert.equal(isRecoverableSqliteWriteError("UNIQUE constraint failed: settings.key"), false);
});

await runTest("createSerializedJobRunner keeps writes strictly ordered", async () => {
  const runSerializedJob = createSerializedJobRunner();
  const events: string[] = [];

  const slowJob = runSerializedJob(async () => {
    events.push("slow:start");
    await new Promise((resolve) => setTimeout(resolve, 10));
    events.push("slow:end");
  });

  const fastJob = runSerializedJob(async () => {
    events.push("fast:start");
    events.push("fast:end");
  });

  await Promise.all([slowJob, fastJob]);

  assert.deepEqual(events, [
    "slow:start",
    "slow:end",
    "fast:start",
    "fast:end",
  ]);
});

console.log(`Passed ${passed} persistence transaction tests`);
