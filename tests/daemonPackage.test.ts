import { execFileSync } from "node:child_process";

execFileSync("python3", ["-B", "scripts/acceptance/daemon-package-tests.py"], { stdio: "inherit" });
console.log("PASS standalone daemon package metadata, integrity, reproducibility and overwrite guards");
