import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const mode = process.argv[2];
if (mode !== "--check" && mode !== "--write") throw new Error("Use --check or --write");
const root = fileURLToPath(new URL("../", import.meta.url));
const output = new URL("../src/platform/protocol/protocol.generated.ts", import.meta.url);
const generated = execFileSync("cargo", [
  "run", "--quiet", "--locked", "--manifest-path", "crates/patina-protocol/Cargo.toml",
  "--features", "typegen", "--example", "typescript",
], { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
if (mode === "--write") {
  writeFileSync(output, generated, "utf8");
  console.log("Generated protocol types");
} else {
  if (readFileSync(output, "utf8") !== generated) {
    throw new Error("Protocol types are stale. Run npm run generate:protocol and review the diff.");
  }
  console.log("Generated protocol types match the Rust contracts");
}
