import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "node:zlib";

const ASSETS_DIR = "dist/assets";
const KI_B = 1024;

const CHUNK_BUDGETS = [
  { label: "charts", pattern: /^charts-.*\.js$/, gzipKiB: 125 },
  { label: "react-vendor", pattern: /^react-vendor-.*\.js$/, gzipKiB: 70 },
  { label: "motion", pattern: /^motion-.*\.js$/, gzipKiB: 50 },
  { label: "icons", pattern: /^icons-.*\.js$/, gzipKiB: 15 },
  { label: "index", pattern: /^index-.*\.js$/, gzipKiB: 71 },
] as const;

const FEATURE_OTHER_CHUNKS_GZIP_BUDGET_KI_B = 70;
const TOTAL_JS_GZIP_BUDGET_KI_B = 370;

function formatKiB(bytes: number) {
  return (bytes / KI_B).toFixed(2);
}

function isNamedBudgetChunk(file: string) {
  return CHUNK_BUDGETS.some((budget) => budget.pattern.test(file));
}

function main() {
  if (!existsSync(ASSETS_DIR)) {
    console.error(`Bundle budget check failed. Missing ${ASSETS_DIR}; run npm run build first.`);
    process.exitCode = 1;
    return;
  }

  const jsAssets = readdirSync(ASSETS_DIR).filter((file) => file.endsWith(".js"));
  const measured = jsAssets.map((file) => {
    const bytes = readFileSync(join(ASSETS_DIR, file));
    return {
      file,
      rawBytes: bytes.length,
      gzipBytes: gzipSync(bytes).length,
    };
  });

  const violations: string[] = [];

  for (const budget of CHUNK_BUDGETS) {
    const asset = measured.find((item) => budget.pattern.test(item.file));
    if (!asset) {
      violations.push(`missing expected ${budget.label} chunk`);
      continue;
    }

    const budgetBytes = budget.gzipKiB * KI_B;
    if (asset.gzipBytes > budgetBytes) {
      violations.push(
        `${budget.label} gzip ${formatKiB(asset.gzipBytes)} KiB exceeds ${budget.gzipKiB} KiB`,
      );
    }
  }

  const featureOtherGzipBytes = measured
    .filter((item) => !isNamedBudgetChunk(item.file))
    .reduce((sum, item) => sum + item.gzipBytes, 0);
  if (featureOtherGzipBytes > FEATURE_OTHER_CHUNKS_GZIP_BUDGET_KI_B * KI_B) {
    violations.push(
      `feature/other chunks gzip ${formatKiB(featureOtherGzipBytes)} KiB exceeds ${FEATURE_OTHER_CHUNKS_GZIP_BUDGET_KI_B} KiB`,
    );
  }

  const totalGzipBytes = measured.reduce((sum, item) => sum + item.gzipBytes, 0);
  if (totalGzipBytes > TOTAL_JS_GZIP_BUDGET_KI_B * KI_B) {
    violations.push(
      `total JS gzip ${formatKiB(totalGzipBytes)} KiB exceeds ${TOTAL_JS_GZIP_BUDGET_KI_B} KiB`,
    );
  }

  if (violations.length > 0) {
    console.error("Bundle budget check failed.");
    for (const violation of violations) {
      console.error(`- ${violation}`);
    }
    process.exitCode = 1;
    return;
  }

  console.log(`Bundle budget check passed. Total JS gzip: ${formatKiB(totalGzipBytes)} KiB`);
  for (const budget of CHUNK_BUDGETS) {
    const asset = measured.find((item) => budget.pattern.test(item.file));
    if (asset) {
      console.log(`${budget.label}: ${formatKiB(asset.gzipBytes)} KiB gzip`);
    }
  }
  console.log(`feature/other chunks: ${formatKiB(featureOtherGzipBytes)} KiB gzip`);
}

main();
