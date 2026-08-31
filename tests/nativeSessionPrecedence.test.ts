import assert from "node:assert/strict";
import { resolveNativeSessionPrecedence } from "../src/platform/persistence/nativeSessionPrecedence.ts";

function duration(records: ReturnType<typeof resolveNativeSessionPrecedence>): number {
  return records.reduce((total, record) => total + record.endTime - record.startTime, 0);
}

{
  const resolved = resolveNativeSessionPrecedence([
    { key: "external", origin: "import_exact", startTime: 0, endTime: 100 },
    { key: "native", origin: "native", startTime: 25, endTime: 75 },
  ]);
  assert.deepEqual(
    resolved.map(({ key, startTime, endTime }) => ({ key, startTime, endTime })),
    [
      { key: "external", startTime: 0, endTime: 25 },
      { key: "native", startTime: 25, endTime: 75 },
      { key: "external", startTime: 75, endTime: 100 },
    ],
  );
}

{
  const resolved = resolveNativeSessionPrecedence([
    { key: "native", origin: "native", startTime: 0, endTime: 30 },
    { key: "a", origin: "import_bucket", startTime: 0, endTime: 60, capacityEndTime: 100 },
    { key: "b", origin: "import_bucket", startTime: 0, endTime: 60, capacityEndTime: 100 },
  ]);
  assert.equal(duration(resolved), 100);
  assert.equal(resolved.find((record) => record.key === "a")?.endTime, 35);
  assert.equal(resolved.find((record) => record.key === "b")?.endTime, 35);
}

{
  const resolved = resolveNativeSessionPrecedence([
    { key: "legacy-native", origin: "native", startTime: 10, endTime: 10 },
    { key: "invalid-import", origin: "import_exact", startTime: 10, endTime: 10 },
  ]);
  assert.deepEqual(
    resolved.map(({ key, startTime, endTime }) => ({ key, startTime, endTime })),
    [{ key: "legacy-native", startTime: 10, endTime: 10 }],
  );
}

console.log("native session precedence tests passed");
