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
  mergeUpdaterManifests,
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
    "https://github.com/Ceceliaee/patina/releases/latest/download/latest.json",
    "https://pub-example.r2.dev/latest.json",
  ]);

  assert.deepEqual(endpoints, [
    "https://github.com/Asanilo/patina/releases/latest/download/latest.json",
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
}

function testReleaseAssetNamesCoverWindowsAndLinuxBundles() {
  assert.deepEqual(releaseAssetNames("1.7.0", "windows-x86_64"), {
    updater: "Patina_1.7.0_x64-setup.exe",
  });
  assert.deepEqual(releaseAssetNames("1.7.0", "linux-x86_64"), {
    updater: "Patina_1.7.0_amd64.AppImage.tar.gz",
    portable: "Patina_1.7.0_amd64.AppImage",
    installer: "Patina_1.7.0_amd64.deb",
  });
}

function testMergeUpdaterManifestsKeepsAllPlatforms() {
  const merged = mergeUpdaterManifests([
    {
      version: "1.7.0",
      notes: "Ready.",
      pub_date: "2026-06-25T00:00:00.000Z",
      platforms: {
        "windows-x86_64": {
          signature: "windows-signature",
          url: "https://example.com/patina.exe",
        },
      },
    },
    {
      version: "1.7.0",
      notes: "Ready.",
      pub_date: "2026-06-25T00:01:00.000Z",
      platforms: {
        "linux-x86_64": {
          signature: "linux-signature",
          url: "https://example.com/patina.AppImage",
        },
      },
    },
  ]);

  assert.equal(merged.version, "1.7.0");
  assert.equal(merged.pub_date, "2026-06-25T00:01:00.000Z");
  assert.deepEqual(Object.keys(merged.platforms).sort(), [
    "linux-x86_64",
    "windows-x86_64",
  ]);
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
  const tauriConfig = JSON.parse(await readFile("src-tauri/tauri.conf.json", "utf8"));
  const { stdout: trackedFirefoxAssets } = await execFileAsync("git", [
    "ls-files",
    "extensions/firefox/dist/patina-web-sync.xpi",
  ]);

  assert.match(workflow, /runs-on: ubuntu-22\.04/);
  assert.match(workflow, /--bundles appimage,deb/);
  assert.match(workflow, /prepare-linux-release-assets/);
  assert.match(workflow, /merge-latest-json/);
  assert.equal(
    trackedFirefoxAssets.trim(),
    "extensions/firefox/dist/patina-web-sync.xpi",
  );
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
  const appImagePath = path.join(bundleDir, "appimage", "Patina_1.7.0_amd64.AppImage");
  const updaterPath = `${appImagePath}.tar.gz`;

  try {
    await mkdir(path.dirname(appImagePath), { recursive: true });
    await mkdir(path.join(bundleDir, "deb"), { recursive: true });
    await writeFile(appImagePath, "appimage", "utf8");
    await writeFile(updaterPath, "updater", "utf8");
    await writeFile(`${updaterPath}.sig`, "linux-signature\n", "utf8");
    await writeFile(
      path.join(bundleDir, "deb", "Patina_1.7.0_amd64.deb"),
      "debian",
      "utf8",
    );

    await execFileAsync(process.execPath, [
      "--experimental-strip-types",
      "scripts/release.ts",
      "prepare-linux-release-assets",
      "1.7.0",
      bundleDir,
      outputDir,
      "Asanilo/patina",
    ]);

    assert.equal(
      await readFile(path.join(outputDir, "Patina_1.7.0_amd64.AppImage"), "utf8"),
      "appimage",
    );
    assert.equal(
      await readFile(
        path.join(outputDir, "Patina_1.7.0_amd64.AppImage.tar.gz"),
        "utf8",
      ),
      "updater",
    );
    assert.equal(
      await readFile(path.join(outputDir, "Patina_1.7.0_amd64.deb"), "utf8"),
      "debian",
    );

    const latest = JSON.parse(
      await readFile(path.join(outputDir, "latest-linux.json"), "utf8"),
    );
    assert.equal(
      latest.platforms["linux-x86_64"].url,
      "https://github.com/Asanilo/patina/releases/download/v1.7.0/Patina_1.7.0_amd64.AppImage.tar.gz",
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
testReleaseAssetNamesCoverWindowsAndLinuxBundles();
testMergeUpdaterManifestsKeepsAllPlatforms();
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

console.log("Passed 20 release policy tests");
