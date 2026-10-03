import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { ProcessMapper } from "../src/shared/classification/processMapper.ts";
import { resolveCanonicalExecutable } from "../src/shared/classification/processNormalization.ts";
import { setUiTextLanguage } from "../src/shared/copy/uiText.ts";

const cases = JSON.parse(readFileSync(new URL("./fixtures/product-classification.json", import.meta.url), "utf8"));
for (const fixture of cases) {
  setUiTextLanguage(fixture.language);
  const overrides = Object.fromEntries(fixture.overrides.map((entry: {exe: string; value: unknown}) => [
    resolveCanonicalExecutable(entry.exe), ProcessMapper.fromOverrideStorageValue(JSON.stringify(entry.value)),
  ]).filter(([, value]: [string, unknown]) => value !== null));
  ProcessMapper.setUserOverrides(overrides);
  ProcessMapper.setDeletedCategories(fixture.deleted);
  for (const check of fixture.checks) {
    assert.equal(ProcessMapper.map(check.exe).category, check.category, fixture.name);
    assert.equal(ProcessMapper.shouldTrack(check.exe), check.tracked, fixture.name);
    assert.equal(ProcessMapper.getUserOverride(check.exe)?.displayName ?? null, check.name, fixture.name);
  }
}
ProcessMapper.clearUserOverrides();
ProcessMapper.setDeletedCategories([]);
setUiTextLanguage("zh-CN");
console.log("Product classification fixtures match the existing Desktop mapper");
