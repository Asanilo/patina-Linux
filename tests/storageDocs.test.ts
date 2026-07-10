import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const product = await readFile("docs/product-principles-and-scope.md", "utf8");
const roadmap = await readFile("docs/roadmap-and-prioritization.md", "utf8");
const setup = await readFile("docs/linux-development-setup.md", "utf8");
const architecture = await readFile("docs/architecture.md", "utf8");

assert.match(product, /自定义本地存储位置/);
assert.match(product, /WebKitCache/);
assert.match(roadmap, /本地存储位置与安全缓存控制已实现/);

assert.match(setup, /\$\{XDG_CONFIG_HOME:-~\/\.config\}\/Patina/);
assert.match(setup, /\$\{XDG_DATA_HOME:-~\/\.local\/share\}\/Patina/);
assert.match(setup, /fail-closed/i);
assert.match(setup, /does not create a database in the default directory/i);
assert.match(setup, /previous source directories.*retained/is);
assert.match(setup, /deletes only.*WebKitCache/is);

assert.match(architecture, /Tauri-free storage bootstrap/);
assert.match(architecture, /pending migration/);
assert.match(setup, /Storage changes use a restart boundary/);

console.log("Validated Linux storage migration documentation contract");
