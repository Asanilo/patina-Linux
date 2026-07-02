import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import {
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import {
  buildUpdaterEndpoints,
  releaseAssetNames,
  fieldValue,
  renderReleaseNotes,
  readVersionPolicyCurrentCodeVersion,
  renderUpdaterNotes,
  syncVersionPolicyCurrentCodeVersion,
  validateReleaseVersionFilesText,
  validateVersionPolicyCurrentCodeVersionText,
} from "../scripts/release.ts";

const execFileAsync = promisify(execFile);
const currentPackageVersion = JSON.parse(
  await readFile("package.json", "utf8"),
).version;

const versionPolicyExcerpt = [
  "## 3. 当前仓库现实",
  "",
  "截至当前仓库状态：",
  "",
  "- 代码版本为 `0.4.2`",
  "- 稳定发布线处于 `0.4.x`",
  "",
].join("\n");

function versionFileFixture(version = "1.6.0") {
  return {
    packageJson: JSON.stringify({ version }),
    packageLockJson: JSON.stringify({
      version,
      packages: {
        "": {
          version,
        },
      },
    }),
    tauriConfig: JSON.stringify({ version }),
    tauriDevConfig: JSON.stringify({ version }),
    tauriLocalConfig: JSON.stringify({ version }),
    cargoToml: [
      "[package]",
      'name = "patina"',
      `version = "${version}"`,
      "",
      "[dependencies]",
    ].join("\n"),
    cargoLock: [
      "version = 4",
      "",
      "[[package]]",
      'name = "other"',
      'version = "0.1.0"',
      "",
      "[[package]]",
      'name = "patina"',
      `version = "${version}"`,
      "dependencies = []",
    ].join("\n"),
    versionPolicy: [
      "## 3. 当前仓库现实",
      "",
      `- 代码版本为 \`${version}\``,
    ].join("\n"),
    changelog: [
      "# Changelog",
      "",
      `## [${version}] - 2026-06-13`,
      "",
      "Release: Ready.",
    ].join("\n"),
  };
}

function testSyncsCurrentCodeVersion() {
  const updated = syncVersionPolicyCurrentCodeVersion(versionPolicyExcerpt, "0.4.3");
  assert.equal(readVersionPolicyCurrentCodeVersion(updated), "0.4.3");
  assert.match(updated, /- 代码版本为 `0\.4\.3`/);
  assert.match(updated, /- 稳定发布线处于 `0\.4\.x`/);
}

function testSupportsPrereleaseVersion() {
  const updated = syncVersionPolicyCurrentCodeVersion(versionPolicyExcerpt, "0.5.0-beta.1");
  assert.equal(readVersionPolicyCurrentCodeVersion(updated), "0.5.0-beta.1");
}

function testMissingPolicyVersionIsNull() {
  assert.equal(readVersionPolicyCurrentCodeVersion("## empty"), null);
}

function testStalePolicyVersionFailsValidation() {
  assert.equal(
    validateVersionPolicyCurrentCodeVersionText(versionPolicyExcerpt, "0.4.3"),
    "docs/versioning-and-release-policy.md current code version is 0.4.2, expected 0.4.3",
  );
}

function testUpdaterNotesKeepLocalizedVariants() {
  const sectionBody = [
    "Release: Fixed release notes.",
    "App note: Fixed Chinese release notes.",
    "App note en: Fixed English release notes.",
  ].join("\n");

  const notes = renderUpdaterNotes({
    appNote: fieldValue(sectionBody, "App note"),
    appNoteEn: fieldValue(sectionBody, "App note en"),
  });

  assert.equal(notes, [
    "zh-CN: Fixed Chinese release notes.",
    "en-US: Fixed English release notes.",
  ].join("\n"));
}

function testUpdaterNotesFallsBackToAppNote() {
  const sectionBody = [
    "Release: Fixed release notes.",
    "App note: Fixed release notes.",
  ].join("\n");

  const notes = renderUpdaterNotes({
    appNote: fieldValue(sectionBody, "App note"),
    appNoteEn: fieldValue(sectionBody, "App note en"),
  });

  assert.equal(notes, "Fixed release notes.");
}

function testUpdaterEndpointsKeepGithubFirstAndPreserveMirrors() {
  const endpoints = buildUpdaterEndpoints([
    "https://pub-example.r2.dev/latest.json",
    "https://github.com/Asanilo/patina/releases/latest/download/latest.json",
    "https://github.com/Ceceliaee/patina/releases/latest/download/latest.json",
    "https://pub-example.r2.dev/latest.json",
  ]);

  assert.deepEqual(endpoints, [
    "https://github.com/Asanilo/patina-Linux/releases/latest/download/latest.json",
    "https://pub-example.r2.dev/latest.json",
  ]);
}

function testReleaseNotesIncludeAllVisibleBullets() {
  const notes = renderReleaseNotes({
    release: "Ready.",
    bullets: Array.from({ length: 8 }, (_, index) => `- Change ${index + 1}`),
  });

  assert.match(notes, /- Change 7/);
  assert.match(notes, /- Change 8/);
  assert.match(notes, /Linux AppImage/);
  assert.match(notes, /Linux Debian/);
  assert.doesNotMatch(notes, /Windows 安装包/);
}

function testReleaseAssetNamesCoverLinuxBundles() {
  assert.deepEqual(releaseAssetNames("1.7.0", "linux-x86_64"), {
    updater: "Patina_1.7.0_amd64.AppImage",
    portable: "Patina_1.7.0_amd64.AppImage",
    installer: "Patina_1.7.0_amd64.deb",
  });
}

function testVersionFilesValidationPassesWhenAllVersionsMatch() {
  assert.deepEqual(validateReleaseVersionFilesText(versionFileFixture(), "1.6.0"), []);
}

function testVersionFilesValidationCatchesPackageJsonMismatch() {
  const files = versionFileFixture();
  files.packageJson = JSON.stringify({ version: "1.5.9" });

  assert.deepEqual(validateReleaseVersionFilesText(files, "1.6.0"), [
    "package.json version is 1.5.9, expected 1.6.0",
  ]);
}

function testVersionFilesValidationCatchesPackageLockRootMismatch() {
  const files = versionFileFixture();
  files.packageLockJson = JSON.stringify({
    version: "1.6.0",
    packages: {
      "": {
        version: "1.5.9",
      },
    },
  });

  assert.deepEqual(validateReleaseVersionFilesText(files, "1.6.0"), [
    'package-lock.json packages[""] version is 1.5.9, expected 1.6.0',
  ]);
}

function testVersionFilesValidationCatchesTauriConfigMismatch() {
  const files = versionFileFixture();
  files.tauriDevConfig = JSON.stringify({ version: "1.5.9" });

  assert.deepEqual(validateReleaseVersionFilesText(files, "1.6.0"), [
    "src-tauri/tauri.dev.conf.json version is 1.5.9, expected 1.6.0",
  ]);
}

function testVersionFilesValidationCatchesCargoMismatch() {
  const files = versionFileFixture();
  files.cargoToml = [
    "[package]",
    'name = "patina"',
    'version = "1.5.9"',
  ].join("\n");
  files.cargoLock = [
    "[[package]]",
    'name = "patina"',
    'version = "1.5.8"',
  ].join("\n");

  assert.deepEqual(validateReleaseVersionFilesText(files, "1.6.0"), [
    "src-tauri/Cargo.toml [package].version is 1.5.9, expected 1.6.0",
    "src-tauri/Cargo.lock package patina version is 1.5.8, expected 1.6.0",
  ]);
}

function testVersionFilesValidationCatchesPolicyMismatch() {
  const files = versionFileFixture();
  files.versionPolicy = versionPolicyExcerpt;

  assert.deepEqual(validateReleaseVersionFilesText(files, "1.6.0"), [
    "docs/versioning-and-release-policy.md current code version is 0.4.2, expected 1.6.0",
  ]);
}

function testVersionFilesValidationCatchesMissingChangelogSection() {
  const files = versionFileFixture();
  files.changelog = "# Changelog\n\n## [1.5.9] - 2026-06-12";

  assert.deepEqual(validateReleaseVersionFilesText(files, "1.6.0"), [
    'CHANGELOG.md is missing "## [1.6.0] - YYYY-MM-DD"',
  ]);
}

function testVersionFilesValidationRejectsInvalidVersion() {
  assert.deepEqual(validateReleaseVersionFilesText(versionFileFixture(), "1.6"), [
    'invalid SemVer version "1.6"',
  ]);
}

async function testLinuxReleaseWorkflowAndBundleContract() {
  const workflow = await readFile(".github/workflows/prepare-release.yml", "utf8");
  const verifyWorkflow = await readFile(".github/workflows/verify.yml", "utf8");
  const tauriConfig = JSON.parse(await readFile("src-tauri/tauri.conf.json", "utf8"));
  const { stdout: trackedFirefoxAssets } = await execFileAsync("git", [
    "ls-files",
    "extensions/firefox/dist/patina-web-sync.xpi",
  ]);

  assert.match(workflow, /runs-on: ubuntu-22\.04/);
  assert.match(workflow, /--bundles appimage,deb/);
  assert.match(workflow, /prepare-linux-release-assets/);
  assert.match(workflow, /Prepare updater signing key/);
  assert.match(workflow, /TAURI_SIGNING_PRIVATE_KEY_PATH=/);
  assert.match(workflow, /printf '%s\\n' "\$TAURI_SIGNING_PRIVATE_KEY" > "\$signing_key_path"/);
  assert.match(workflow, /chmod 600 "\$signing_key_path"/);
  assert.match(workflow, /Cleanup updater signing key/);
  assert.match(workflow, /rm -f "\$RUNNER_TEMP\/tauri-signing\.key"/);
  assert.match(workflow, /Package Chromium extension/);
  assert.match(workflow, /npm run extension:firefox:verify-signed/);
  assert.match(workflow, /Publish Linux release/);
  assert.doesNotMatch(
    workflow,
    /Build AppImage and Debian bundles[\s\S]*TAURI_SIGNING_PRIVATE_KEY:\s*\$\{\{\s*secrets\.TAURI_SIGNING_PRIVATE_KEY\s*\}\}/,
  );
  assert.doesNotMatch(workflow, /windows-latest/);
  assert.doesNotMatch(workflow, /--bundles nsis/);
  assert.doesNotMatch(workflow, /windows-x86_64/);
  assert.doesNotMatch(workflow, /merge-latest-json/);
  assert.match(verifyWorkflow, /runs-on: ubuntu-22\.04/);
  assert.match(verifyWorkflow, /workflow_dispatch:/);
  assert.doesNotMatch(verifyWorkflow, /windows-latest/);
  assert.equal(
    trackedFirefoxAssets.trim(),
    "extensions/firefox/dist/patina-web-sync.xpi",
  );
  assert.deepEqual(tauriConfig.plugins.updater.endpoints, [
    "https://github.com/Asanilo/patina-Linux/releases/latest/download/latest.json",
  ]);
  assert.equal(
    tauriConfig.bundle.linux.deb.files[
      "/usr/share/gnome-shell/extensions/patina-window-tracker@patina/extension.js"
    ],
    "../extensions/gnome-shell/patina-window-tracker@patina/extension.js",
  );
}

async function testPrepareLinuxReleaseAssetsCreatesInstallerAndUpdaterManifest() {
  const tempRoot = await mkdtemp(path.join(tmpdir(), "patina-linux-release-"));
  const bundleDir = path.join(tempRoot, "bundle");
  const outputDir = path.join(tempRoot, "output");
  const appImageName = `Patina_${currentPackageVersion}_amd64.AppImage`;
  const appImagePath = path.join(bundleDir, "appimage", appImageName);
  const debName = `Patina_${currentPackageVersion}_amd64.deb`;

  try {
    await mkdir(path.dirname(appImagePath), { recursive: true });
    await mkdir(path.join(bundleDir, "deb"), { recursive: true });
    await writeFile(appImagePath, "appimage", "utf8");
    await writeFile(`${appImagePath}.sig`, "linux-signature\n", "utf8");
    await writeFile(
      path.join(bundleDir, "deb", debName),
      "debian",
      "utf8",
    );

    await execFileAsync(process.execPath, [
      "--experimental-strip-types",
      "scripts/release.ts",
      "prepare-linux-release-assets",
      currentPackageVersion,
      bundleDir,
      outputDir,
      "Asanilo/patina-Linux",
    ]);

    assert.equal(
      await readFile(path.join(outputDir, appImageName), "utf8"),
      "appimage",
    );
    assert.equal(
      await readFile(path.join(outputDir, debName), "utf8"),
      "debian",
    );

    const latest = JSON.parse(
      await readFile(path.join(outputDir, "latest.json"), "utf8"),
    );
    assert.equal(
      latest.platforms["linux-x86_64"].url,
      `https://github.com/Asanilo/patina-Linux/releases/download/v${currentPackageVersion}/Patina_${currentPackageVersion}_amd64.AppImage`,
    );
    assert.equal(
      latest.platforms["linux-x86_64"].signature,
      "linux-signature",
    );
  } finally {
    await rm(tempRoot, { force: true, recursive: true });
  }
}

testSyncsCurrentCodeVersion();
testSupportsPrereleaseVersion();
testMissingPolicyVersionIsNull();
testStalePolicyVersionFailsValidation();
testUpdaterNotesKeepLocalizedVariants();
testUpdaterNotesFallsBackToAppNote();
testUpdaterEndpointsKeepGithubFirstAndPreserveMirrors();
testReleaseNotesIncludeAllVisibleBullets();
testReleaseAssetNamesCoverLinuxBundles();
testVersionFilesValidationPassesWhenAllVersionsMatch();
testVersionFilesValidationCatchesPackageJsonMismatch();
testVersionFilesValidationCatchesPackageLockRootMismatch();
testVersionFilesValidationCatchesTauriConfigMismatch();
testVersionFilesValidationCatchesCargoMismatch();
testVersionFilesValidationCatchesPolicyMismatch();
testVersionFilesValidationCatchesMissingChangelogSection();
testVersionFilesValidationRejectsInvalidVersion();
await testLinuxReleaseWorkflowAndBundleContract();
await testPrepareLinuxReleaseAssetsCreatesInstallerAndUpdaterManifest();

console.log("Passed 19 release policy tests");
