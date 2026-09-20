import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import { join, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";

interface TestException {
  reason: string;
  command: string;
}

// Every tests/**/*.test.ts runs by default. Any real-environment/manual exception
// must name one exact file, explain its requirements and give its explicit command.
// Native lifecycle/performance tools under scripts/ and Rust ignored tests retain
// their separate opt-in commands; no current TypeScript test needs an exception.
const TEST_EXCEPTIONS: Record<string, TestException> = {};

export function discoverTests(root: string): string[] {
  return readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const path = join(root, entry.name);
    if (entry.isDirectory()) return discoverTests(path);
    return entry.isFile() && entry.name.endsWith(".test.ts")
      ? [path.split(sep).join("/")]
      : [];
  }).sort();
}

export function selectTests(files: string[], exceptions: Record<string, TestException> = TEST_EXCEPTIONS) {
  if (files.length === 0) throw new Error("No TypeScript tests found");
  for (const [file, exception] of Object.entries(exceptions)) {
    if (!files.includes(file)) throw new Error(`Stale or non-exact test exception: ${file}`);
    if (!exception.reason.trim() || !exception.command.trim()) {
      throw new Error(`Test exception needs a reason and explicit command: ${file}`);
    }
  }
  const selected = files.filter((file) => !Object.hasOwn(exceptions, file));
  if (selected.length === 0) throw new Error("No TypeScript tests remain after explicit exceptions");
  return selected;
}

export function runTests(files: string[]): number {
  for (const file of files) {
    console.log(`\nRunning ${file}`);
    const result = spawnSync(process.execPath, [
      "--experimental-strip-types",
      "--experimental-specifier-resolution=node",
      file,
    ], { stdio: "inherit" });
    if (result.error || result.status !== 0) {
      console.error(`Test failed: ${file}`, result.error ?? result.signal ?? result.status);
      return result.error ? 1 : result.status ?? 1;
    }
  }
  console.log(`\nPassed ${files.length} TypeScript test files`);
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const files = selectTests(discoverTests("tests"));
  for (const [file, exception] of Object.entries(TEST_EXCEPTIONS)) {
    console.log(`Excluded ${file}: ${exception.reason}; run: ${exception.command}`);
  }
  process.exitCode = runTests(files);
}
