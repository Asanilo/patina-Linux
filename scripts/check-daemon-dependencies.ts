import { execFileSync } from "node:child_process";

// The no-default-features projection must stay free of desktop dependencies,
// including dependencies reached through other crates and build scripts.
const tree = execFileSync("cargo", [
  "tree", "--locked", "--manifest-path", "src-tauri/Cargo.toml",
  "--no-default-features", "--edges", "normal,build", "--prefix", "none",
  "--format", "{p}",
], { encoding: "utf8" });
const forbidden = /^(?:tauri(?:-.*)?|gtk\d?(?:-.*)?|gdk\d?(?:-.*)?|webkit.*|wry|tao|rfd|glib(?:-.*)?|gio(?:-.*)?|pango(?:-.*)?|cairo(?:-.*)?|ts-rs(?:-.*)?)$/;
const found = [...new Set(tree.split(/\r?\n/).map((line) => line.split(" ")[0]))]
  .filter((name) => forbidden.test(name));
if (found.length) {
  throw new Error(`Independent daemon must not depend on desktop crates: ${found.join(", ")}`);
}
console.log("Independent daemon dependency boundary passed");
