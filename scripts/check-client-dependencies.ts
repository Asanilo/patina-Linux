import { execFileSync } from "node:child_process";

// Inspect the resolved graph, not only direct Cargo.toml entries: an indirect
// dependency on the product or a UI crate also defeats independent clients.
const tree = execFileSync("cargo", [
  "tree", "--locked", "--manifest-path", "crates/patina-client/Cargo.toml",
  "--edges", "normal,build", "--prefix", "none", "--format", "{p}",
], { encoding: "utf8" });
const forbidden = /^(?:patina|tauri(?:-.*)?|gtk\d?(?:-.*)?|gdk\d?(?:-.*)?|webkit.*|sqlx(?:-.*)?|libsqlite3-sys|gpui(?:-.*)?|ratatui|crossterm)$/;
const found = [...new Set(tree.split(/\r?\n/).map((line) => line.split(" ")[0]))]
  .filter((name) => forbidden.test(name));
if (found.length) {
  throw new Error(`patina-client must not depend on runtime/storage/UI crates: ${found.join(", ")}`);
}
console.log("Independent client dependency boundary passed");
