import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";

const SCAN_ROOTS = [
  "src-tauri/src/commands",
  "src-tauri/src/app",
  "src-tauri/src/platform",
  "src-tauri/src/domain",
] as const;

const EXTRA_FILES = ["src-tauri/src/lib.rs"] as const;

const STORAGE_PATH_OWNER_FILES = new Set([
  "src-tauri/src/data/sqlite_pool.rs",
  "src-tauri/src/data/backup.rs",
  "src-tauri/src/data/remote_backup.rs",
  "src-tauri/src/app/main_window.rs",
  "src-tauri/src/app/widget.rs",
]);

const STORAGE_PATH_OWNER_EXTRA_FILES = [...STORAGE_PATH_OWNER_FILES].filter(
  (path) => !path.startsWith("src-tauri/src/app/"),
);

interface SourceFile {
  path: string;
  content: string;
}

interface BoundaryViolation {
  path: string;
  line: number;
  rule: string;
  text: string;
}

function normalizePath(path: string) {
  return path.split(sep).join("/");
}

function collectRustFiles(root: string): SourceFile[] {
  const files: SourceFile[] = [];

  function walk(path: string) {
    const stats = statSync(path);
    if (stats.isDirectory()) {
      for (const entry of readdirSync(path)) {
        walk(join(path, entry));
      }
      return;
    }

    if (!path.endsWith(".rs")) {
      return;
    }

    files.push({
      path: normalizePath(relative(process.cwd(), path)),
      content: readFileSync(path, "utf8"),
    });
  }

  walk(root);
  return files;
}

function isCommandsSource(path: string) {
  return /^src-tauri\/src\/commands\//.test(path);
}

function isAppSource(path: string) {
  return /^src-tauri\/src\/app\//.test(path);
}

function isPlatformSource(path: string) {
  return /^src-tauri\/src\/platform\//.test(path);
}

function isDomainSource(path: string) {
  return /^src-tauri\/src\/domain\//.test(path);
}

function isLibSource(path: string) {
  return path === "src-tauri/src/lib.rs";
}

function isTestLine(lineText: string) {
  return lineText.includes("#[cfg(test)]") || lineText.trim().startsWith("mod tests");
}

function findRustBoundaryViolations(files: SourceFile[]): BoundaryViolation[] {
  const violations: BoundaryViolation[] = [];

  for (const file of files) {
    let inTestModule = false;
    const lines = file.content.split(/\r?\n/);

    lines.forEach((lineText, index) => {
      if (isTestLine(lineText)) {
        inTestModule = true;
      }

      const line = lineText.trim();
      const isSqlQuery = /\bsqlx::query(?:_scalar|_as)?(?:\s*::\s*<[^>]+>)?\s*\(/.test(line);

      if ((isCommandsSource(file.path) || isAppSource(file.path) || isLibSource(file.path)) && isSqlQuery) {
        violations.push({
          path: file.path,
          line: index + 1,
          rule: "entry-layer-no-direct-sql-query",
          text: line,
        });
      }

      if (isCommandsSource(file.path) && /\bPool\s*<\s*Sqlite\s*>/.test(line)) {
        violations.push({
          path: file.path,
          line: index + 1,
          rule: "commands-no-sqlite-pool-type",
          text: line,
        });
      }

      if (isPlatformSource(file.path) && line.includes("crate::data::")) {
        violations.push({
          path: file.path,
          line: index + 1,
          rule: "platform-no-data-import",
          text: line,
        });
      }

      if (isDomainSource(file.path) && !inTestModule && line.includes("crate::data::")) {
        violations.push({
          path: file.path,
          line: index + 1,
          rule: "domain-no-data-import",
          text: line,
        });
      }

      if (isDomainSource(file.path) && !inTestModule && line.includes("crate::platform::")) {
        violations.push({
          path: file.path,
          line: index + 1,
          rule: "domain-no-platform-import",
          text: line,
        });
      }

      if (
        STORAGE_PATH_OWNER_FILES.has(file.path) &&
        /app_paths::product_(?:roaming|local|webview)_data_dir/.test(line)
      ) {
        violations.push({
          path: file.path,
          line: index + 1,
          rule: "persistent-owner-must-use-storage-paths",
          text: line,
        });
      }
    });
  }

  return violations;
}

function runSelfTest() {
  const violations = findRustBoundaryViolations([
    {
      path: "src-tauri/src/commands/tracking.rs",
      content: "let row = sqlx::query(\"SELECT 1\");\nfn takes_pool(pool: Pool<Sqlite>) {}",
    },
    {
      path: "src-tauri/src/app/bootstrap.rs",
      content: "let row = sqlx::query_scalar(\"SELECT 1\");",
    },
    {
      path: "src-tauri/src/lib.rs",
      content: "let row = sqlx::query_as::<_, Row>(\"SELECT 1\");",
    },
    {
      path: "src-tauri/src/platform/windows/foo.rs",
      content: "use crate::data::sqlite_pool::wait_for_sqlite_pool;",
    },
    {
      path: "src-tauri/src/domain/tracking.rs",
      content: "use crate::platform::windows::foreground;\nuse crate::data::schema;",
    },
    {
      path: "src-tauri/src/data/sqlite_pool.rs",
      content:
        "let row = sqlx::query(\"SELECT 1\");\nlet root = app_paths::product_roaming_data_dir(app)?;",
    },
  ]);

  const rules = violations.map((violation) => violation.rule).sort();
  const expectedRules = [
    "commands-no-sqlite-pool-type",
    "domain-no-data-import",
    "domain-no-platform-import",
    "entry-layer-no-direct-sql-query",
    "entry-layer-no-direct-sql-query",
    "entry-layer-no-direct-sql-query",
    "platform-no-data-import",
    "persistent-owner-must-use-storage-paths",
  ].sort();

  if (JSON.stringify(rules) !== JSON.stringify(expectedRules)) {
    throw new Error("Rust boundary self-test failed");
  }
}

function main() {
  if (process.argv.includes("--self-test")) {
    runSelfTest();
    console.log("Rust boundary self-test passed");
    return;
  }

  const files = [
    ...SCAN_ROOTS.flatMap((root) => collectRustFiles(root)),
    ...[...EXTRA_FILES, ...STORAGE_PATH_OWNER_EXTRA_FILES].map((path) => ({
      path,
      content: readFileSync(path, "utf8"),
    })),
  ];
  const violations = findRustBoundaryViolations(files);

  if (violations.length === 0) {
    console.log("Rust boundary check passed");
    return;
  }

  console.error("Rust boundary check failed. Entry, platform, and domain layers must stay thin.");
  for (const violation of violations) {
    console.error(`${violation.path}:${violation.line} ${violation.rule} -> ${violation.text}`);
  }
  process.exitCode = 1;
}

main();
