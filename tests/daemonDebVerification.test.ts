import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  validateDaemonDebMetadata,
  validateExtractedDaemonDeb,
} from "../scripts/verify-daemon-deb.ts";

let passed = 0;

async function runTest(name: string, fn: () => Promise<void> | void) {
  try {
    await fn();
    passed += 1;
    console.log(`PASS ${name}`);
  } catch (error) {
    console.error(`FAIL ${name}`);
    console.error(error);
    process.exitCode = 1;
  }
}

async function createPayloadFixture(root: string) {
  const files = new Map<string, string>([
    ["usr/bin/Patina", "desktop-binary"],
    ["usr/bin/patinad", "daemon-binary"],
    ["usr/lib/systemd/user/patinad.service", [
      "[Unit]",
      "Description=Patina local activity tracking runtime",
      "[Service]",
      "ExecStart=/usr/bin/patinad --profile production --serve-api --track",
      "Environment=PATINA_SYSTEMD_SERVICE=patinad.service",
      "Restart=on-failure",
      "KillSignal=SIGINT",
      "UMask=0077",
      "NoNewPrivileges=true",
      "ProtectSystem=strict",
      "[Install]",
      "WantedBy=default.target",
    ].join("\n")],
    ["usr/share/gnome-shell/extensions/patina-window-tracker@patina/extension.js", "export default class {}"],
    ["usr/share/gnome-shell/extensions/patina-window-tracker@patina/metadata.json", JSON.stringify({
      uuid: "patina-window-tracker@patina",
    })],
  ]);

  for (const [relativePath, content] of files) {
    const filePath = path.join(root, relativePath);
    await mkdir(path.dirname(filePath), { recursive: true });
    await writeFile(filePath, content, "utf8");
    await chmod(filePath, relativePath.startsWith("usr/bin/") ? 0o755 : 0o644);
  }
}

await runTest("daemon DEB metadata requires the Patina amd64 release identity", () => {
  assert.deepEqual(validateDaemonDebMetadata({
    packageName: "patina",
    version: "1.8.4-beta.1",
    architecture: "amd64",
  }, "1.8.4-beta.1"), []);
  assert.deepEqual(validateDaemonDebMetadata({
    packageName: "other",
    version: "1.8.3",
    architecture: "arm64",
  }, "1.8.4-beta.1"), [
    "package name is other, expected patina",
    "package version is 1.8.3, expected 1.8.4-beta.1",
    "package architecture is arm64, expected amd64",
  ]);
});

await runTest("daemon DEB payload accepts the complete default-disabled product layout", async () => {
  const root = await mkdtemp(path.join(tmpdir(), "patina-daemon-deb-fixture-"));
  try {
    await createPayloadFixture(root);
    assert.deepEqual(await validateExtractedDaemonDeb(root), []);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

await runTest("daemon DEB payload rejects missing files and unsafe service behavior", async () => {
  const root = await mkdtemp(path.join(tmpdir(), "patina-daemon-deb-invalid-"));
  try {
    await createPayloadFixture(root);
    const daemonPath = path.join(root, "usr/bin/patinad");
    await chmod(daemonPath, 0o644);
    await writeFile(
      path.join(root, "usr/lib/systemd/user/patinad.service"),
      "[Service]\nExecStart=/usr/bin/patinad\nsystemctl enable --now patinad.service\n",
      "utf8",
    );
    await writeFile(
      path.join(root, "usr/share/gnome-shell/extensions/patina-window-tracker@patina/metadata.json"),
      JSON.stringify({ uuid: "wrong@example" }),
      "utf8",
    );
    const enabledPath = path.join(
      root,
      "usr/lib/systemd/user/default.target.wants/patinad.service",
    );
    await mkdir(path.dirname(enabledPath), { recursive: true });
    await writeFile(enabledPath, "premature enable", "utf8");

    const errors = await validateExtractedDaemonDeb(root);
    assert.ok(errors.some((error) => error.includes("usr/bin/patinad must be executable")));
    assert.ok(errors.some((error) => error.includes("must not recursively enable")));
    assert.ok(errors.some((error) => error.includes("GNOME extension UUID")));
    assert.ok(errors.some((error) => error.includes("must not be packaged")));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

if (!process.exitCode) {
  console.log(`${passed} daemon DEB verification tests passed`);
}
