import assert from "node:assert/strict";
import {
  parseActivityImportBatches,
  parseActivityImportPreview,
  parseActivityImportReport,
} from "../src/platform/persistence/activityImportRuntimeGateway.ts";

const preview = parseActivityImportPreview({
  filePath: "/tmp/activity.csv",
  fileName: "activity.csv",
  fileFingerprint: "a".repeat(64),
  validRecords: 3,
  duplicateRecords: 1,
  errorRecords: 1,
  exactSessions: 2,
  hourBuckets: 1,
  errors: [{ line: 4, message: "invalid row" }],
});
assert.equal(preview.errors[0].line, 4);

const report = parseActivityImportReport({
  batchId: "import-1",
  importedRecords: 2,
  duplicateRecords: 1,
  errorRecords: 0,
  exactSessions: 1,
  hourBuckets: 1,
});
assert.equal(report.batchId, "import-1");

const batches = parseActivityImportBatches([{
  id: "import-1",
  importedAt: 1_714_000_000_000,
  sourceName: "activity.csv",
  sourceKind: "patina-csv",
  exactSessions: 1,
  hourBuckets: 1,
  totalRecords: 2,
}]);
assert.equal(batches[0].totalRecords, 2);

assert.throws(
  () => parseActivityImportPreview({ ...preview, validRecords: "3" }),
  /invalid activity import preview/,
);
assert.throws(
  () => parseActivityImportBatches([{ ...batches[0], importedAt: Number.NaN }]),
  /invalid activity import batches/,
);

console.log("PASS activity import runtime gateway payload validation");
