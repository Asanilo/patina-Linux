import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import {
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import {
  buildLinuxUpdaterPlatforms,
  buildUpdaterEndpoints,
  releaseAssetNames,
  fieldValue,
  isDebOnlyBeta,
  guardReleaseAssets,
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
const stableFixtureVersion = "1.8.4";
const debOnlyFixtureVersion = "1.9.0-beta.21";
const releaseScriptPath = path.resolve("scripts/release.ts");

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
    version: "1.8.4",
    release: "Ready.",
    bullets: Array.from({ length: 8 }, (_, index) => `- Change ${index + 1}`),
  });

  assert.match(notes, /- Change 7/);
  assert.match(notes, /- Change 8/);
  assert.match(notes, /Linux AppImage/);
  assert.match(notes, /Linux Debian/);
  assert.doesNotMatch(notes, /Windows 安装包/);
}

function testDaemonBackedPrereleaseNotesOnlyOfferDebian() {
  const notes = renderReleaseNotes({
    version: "1.9.0-beta.1",
    release: "Ready for daemon validation.",
    bullets: [],
  });

  assert.equal(isDebOnlyBeta("1.9.0-beta.1"), true);
  assert.equal(isDebOnlyBeta("1.9.0-rc.1"), false);
  assert.equal(isDebOnlyBeta("1.9.0"), false);
  assert.match(notes, /Linux Debian beta/);
  assert.doesNotMatch(notes, /Linux AppImage/);
}

function testUpdaterPlatformsKeepStableAndPrereleaseContractsSeparate() {
  const stablePlatforms = buildLinuxUpdaterPlatforms({
    version: "1.8.4",
    repository: "Asanilo/patina-Linux",
    appImageSignature: "appimage-signature",
    debSignature: "deb-signature",
  });
  const prereleasePlatforms = buildLinuxUpdaterPlatforms({
    version: "1.9.0-beta.1",
    repository: "Asanilo/patina-Linux",
    debSignature: "deb-signature",
  });
  const candidatePlatforms = buildLinuxUpdaterPlatforms({
    version: "1.9.0-rc.1",
    repository: "Asanilo/patina-Linux",
    appImageSignature: "appimage-signature",
    debSignature: "deb-signature",
  });

  assert.deepEqual(Object.keys(stablePlatforms), [
    "linux-x86_64",
    "linux-x86_64-appimage",
    "linux-x86_64-deb",
  ]);
  assert.deepEqual(prereleasePlatforms, {
    "linux-x86_64-deb": {
      signature: "deb-signature",
      url: "https://github.com/Asanilo/patina-Linux/releases/download/v1.9.0-beta.1/Patina_1.9.0-beta.1_amd64.deb",
    },
  });
  assert.deepEqual(Object.keys(candidatePlatforms), [
    "linux-x86_64",
    "linux-x86_64-appimage",
    "linux-x86_64-deb",
  ]);
  assert.match(candidatePlatforms["linux-x86_64"].url, /_amd64\.AppImage$/);
  assert.match(candidatePlatforms["linux-x86_64-deb"].url, /_amd64\.deb$/);
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
  const readme = await readFile("README.md", "utf8");
  const chineseReadme = await readFile("README.zh-CN.md", "utf8");
  const linuxSetup = await readFile("docs/linux-development-setup.md", "utf8");
  const versionPolicy = await readFile("docs/versioning-and-release-policy.md", "utf8");
  const packageJson = JSON.parse(await readFile("package.json", "utf8"));
  const cargoManifest = await readFile("src-tauri/Cargo.toml", "utf8");
  const daemonUnit = await readFile("packaging/systemd/patinad.service", "utf8");
  const tauriConfig = JSON.parse(await readFile("src-tauri/tauri.conf.json", "utf8"));
  assert.equal(tauriConfig.bundle.linux.appimage.files["/usr/bin/patinad"], "target/release/patinad");
  const { stdout: trackedFirefoxAssets } = await execFileAsync("git", [
    "ls-files",
    "extensions/firefox/dist/patina-web-sync.xpi",
  ]);

  assert.match(workflow, /runs-on: ubuntu-22\.04/);
  assert.match(workflow, /bundle_targets=appimage,deb/);
  assert.match(workflow, /bundle_targets=deb/);
  assert.match(workflow, /--bundles "\$\{\{ steps\.release\.outputs\.bundle_targets \}\}"/);
  assert.match(workflow, /prepare-linux-release-assets/);
  assert.match(workflow, /Prepare updater signing key/);
  assert.match(workflow, /Build pinned signature verifier/);
  assert.match(workflow, /signing-preflight\.py/);
  assert.match(workflow, /Verify updater bundle signatures/);
  assert.match(workflow, /verify-release-bundles\.py/);
  assert.match(workflow, /TAURI_SIGNING_PRIVATE_KEY_PATH=/);
  assert.match(workflow, /normalize-signing-key\.py > "\$signing_key_path"/);
  assert.match(workflow, /chmod 600 "\$signing_key_path"/);
  assert.match(workflow, /Cleanup updater signing key/);
  assert.match(workflow, /rm -f "\$RUNNER_TEMP\/tauri-signing\.key"/);
  assert.match(workflow, /Package Chromium extension/);
  assert.match(workflow, /Package modern GNOME Shell extension/);
  assert.match(workflow, /extension:gnome:build-esm/);
  assert.match(workflow, /cd dist\/extensions\/gnome-shell\/patina-window-tracker@patina\s+zip -j [^\n]+ metadata\.json extension\.js/);
  assert.match(workflow, /cd dist\/extensions\/gnome-shell-esm\/patina-window-tracker@patina\s+zip -j [^\n]+ metadata\.json extension\.js/);
  assert.match(workflow, /npm run extension:firefox:verify-signed/);
  assert.match(workflow, /Verify daemon-backed Debian package/);
  assert.match(workflow, /npm run release:verify-daemon-deb/);
  assert.match(workflow, /Publish stable Linux release/);
  assert.match(workflow, /Publish daemon-backed Linux beta/);
  assert.match(workflow, /Publish dual-bundle Linux release candidate/);
  const stablePublishBlock = workflow.slice(
    workflow.indexOf("- name: Publish stable Linux release"),
    workflow.indexOf("- name: Publish daemon-backed Linux beta"),
  );
  const assetGuardBlock = workflow.slice(
    workflow.indexOf("- name: Guard existing release assets"),
    workflow.indexOf("- name: Publish stable Linux release"),
  );
  assert.match(assetGuardBlock, /guard-release-assets/);
  assert.match(assetGuardBlock, /if \[\[ "\$DEB_ONLY" != 'true' \]\]; then[\s\S]*_amd64\.AppImage[\s\S]*GNOME_ESM_EXTENSION_ASSET/);
  assert.match(assetGuardBlock, /_amd64\.deb/);
  assert.match(assetGuardBlock, /latest\.json/);
  assert.match(workflow, /group: linux-release-/);
  assert.match(workflow, /cancel-in-progress: false/);
  assert.match(workflow, /deb_only=true/);
  assert.match(workflow, /deb_only=false/);
  const prereleasePublishBlock = workflow.slice(
    workflow.indexOf("- name: Publish daemon-backed Linux beta"),
    workflow.indexOf("- name: Publish dual-bundle Linux release candidate"),
  );
  assert.doesNotMatch(prereleasePublishBlock, /\.amd64\.AppImage/);
  assert.doesNotMatch(prereleasePublishBlock, /GNOME_ESM_EXTENSION_ASSET/);
  assert.match(prereleasePublishBlock, /prerelease: true/);
  const candidatePublishBlock = workflow.slice(
    workflow.indexOf("- name: Publish dual-bundle Linux release candidate"),
  );
  assert.match(candidatePublishBlock, /prerelease: true/);
  for (const block of [stablePublishBlock, prereleasePublishBlock, candidatePublishBlock]) {
    assert.match(block, /steps\.asset_guard\.outputs\.publish == 'true'/);
    assert.match(block, /files: \$\{\{ steps\.asset_guard\.outputs\.files \}\}/);
    assert.match(block, /overwrite_files: false/);
    assert.match(block, /fail_on_unmatched_files: true/);
  }
  assert.doesNotMatch(workflow, /overwrite_files: true/);
  assert.doesNotMatch(
    workflow,
    /Build Linux bundles[\s\S]*TAURI_SIGNING_PRIVATE_KEY:\s*\$\{\{\s*secrets\.TAURI_SIGNING_PRIVATE_KEY\s*\}\}/,
  );
  assert.doesNotMatch(workflow, /windows-latest/);
  assert.doesNotMatch(workflow, /--bundles nsis/);
  assert.doesNotMatch(workflow, /windows-x86_64/);
  assert.doesNotMatch(workflow, /merge-latest-json/);
  assert.doesNotMatch(readme, /Patina_<version>_amd64\.AppImage\.tar\.gz/);
  assert.doesNotMatch(chineseReadme, /Patina_<version>_amd64\.AppImage\.tar\.gz/);
  assert.doesNotMatch(linuxSetup, /\.deb` remains the Debian \/ Ubuntu manual installation path/);
  assert.match(linuxSetup, /linux-x86_64-appimage/);
  assert.match(linuxSetup, /linux-x86_64-deb/);
  assert.match(versionPolicy, /linux-x86_64-appimage/);
  assert.match(versionPolicy, /linux-x86_64-deb/);
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
  assert.match(
    tauriConfig.build.beforeBuildCommand,
    /npm run build:patinad:release/,
  );
  assert.equal(
    packageJson.scripts["build:patinad:release"],
    "cargo build --manifest-path src-tauri/Cargo.toml --release --bin patinad",
  );
  assert.equal(
    packageJson.scripts["release:verify-daemon-deb"],
    "node --experimental-strip-types scripts/verify-daemon-deb.ts",
  );
  assert.equal(
    packageJson.scripts["release:inspect-installed-patinad"],
    "node --experimental-strip-types scripts/patinad-installed-acceptance.ts",
  );
  assert.match(cargoManifest, /^default-run = "patina"$/m);
  assert.equal(
    tauriConfig.bundle.linux.deb.files["/usr/bin/patinad"],
    "target/release/patinad",
  );
  assert.equal(
    tauriConfig.bundle.linux.deb.files["/usr/lib/systemd/user/patinad.service"],
    "../packaging/systemd/patinad.service",
  );
  assert.match(daemonUnit, /^ExecStart=\/usr\/bin\/patinad --profile production --serve-api --track$/m);
  assert.match(daemonUnit, /^Environment=PATINA_SYSTEMD_SERVICE=patinad\.service$/m);
  assert.match(daemonUnit, /^Restart=on-failure$/m);
  assert.match(daemonUnit, /^KillSignal=SIGINT$/m);
  assert.match(daemonUnit, /^WantedBy=default\.target$/m);
  assert.doesNotMatch(daemonUnit, /systemctl|enable --now/);
  assert.equal(
    tauriConfig.bundle.linux.deb.files[
      "/usr/share/gnome-shell/extensions/patina-window-tracker@patina/extension.js"
    ],
    "../extensions/gnome-shell/patina-window-tracker@patina/extension.js",
  );
}

async function testPrepareStableLinuxReleaseAssetsCreatesBothPackageTargets(version = stableFixtureVersion) {
  const tempRoot = await mkdtemp(path.join(tmpdir(), "patina-linux-release-"));
  const bundleDir = path.join(tempRoot, "bundle");
  const outputDir = path.join(tempRoot, "output");
  const appImageName = `Patina_${version}_amd64.AppImage`;
  const appImagePath = path.join(bundleDir, "appimage", appImageName);
  const debName = `Patina_${version}_amd64.deb`;

  try {
    await mkdir(path.dirname(appImagePath), { recursive: true });
    await mkdir(path.join(bundleDir, "deb"), { recursive: true });
    await mkdir(path.join(tempRoot, "docs"), { recursive: true });
    await writeFile(path.join(tempRoot, "CHANGELOG.md"), [
      "# Changelog",
      "",
      `## [${version}] - 2026-08-30`,
      "",
      "Release: Stable fixture.",
      "App note: Stable fixture.",
      "App note en: Stable fixture.",
    ].join("\n"), "utf8");
    await writeFile(
      path.join(tempRoot, "docs", "versioning-and-release-policy.md"),
      `- 代码版本为 \`${version}\`\n`,
      "utf8",
    );
    await writeFile(appImagePath, "appimage", "utf8");
    await writeFile(`${appImagePath}.sig`, "appimage-signature\n", "utf8");
    await writeFile(
      path.join(bundleDir, "deb", debName),
      "debian",
      "utf8",
    );
    await writeFile(
      path.join(bundleDir, "deb", `${debName}.sig`),
      "deb-signature\n",
      "utf8",
    );

    await execFileAsync(process.execPath, [
      "--experimental-strip-types",
      releaseScriptPath,
      "prepare-linux-release-assets",
      version,
      bundleDir,
      outputDir,
      "Asanilo/patina-Linux",
    ], { cwd: tempRoot });

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
    const appImageUrl =
      `https://github.com/Asanilo/patina-Linux/releases/download/v${version}/Patina_${version}_amd64.AppImage`;
    const debUrl =
      `https://github.com/Asanilo/patina-Linux/releases/download/v${version}/Patina_${version}_amd64.deb`;
    assert.deepEqual(latest.platforms["linux-x86_64"], {
      signature: "appimage-signature",
      url: appImageUrl,
    });
    assert.deepEqual(latest.platforms["linux-x86_64-appimage"], {
      signature: "appimage-signature",
      url: appImageUrl,
    });
    assert.deepEqual(latest.platforms["linux-x86_64-deb"], {
      signature: "deb-signature",
      url: debUrl,
    });
  } finally {
    await rm(tempRoot, { force: true, recursive: true });
  }
}

async function testPreparePrereleaseLinuxAssetsCreatesOnlyDebianTarget() {
  const tempRoot = await mkdtemp(path.join(tmpdir(), "patina-linux-prerelease-"));
  const bundleDir = path.join(tempRoot, "bundle");
  const outputDir = path.join(tempRoot, "output");
  const debName = `Patina_${debOnlyFixtureVersion}_amd64.deb`;
  const debPath = path.join(bundleDir, "deb", debName);

  try {
    await mkdir(path.dirname(debPath), { recursive: true });
    await mkdir(path.join(tempRoot, "docs"), { recursive: true });
    await writeFile(path.join(tempRoot, "CHANGELOG.md"), [
      "# Changelog",
      "",
      `## [${debOnlyFixtureVersion}] - 2026-09-23`,
      "",
      "Release: Debian beta fixture.",
      "App note: Debian beta fixture.",
      "App note en: Debian beta fixture.",
    ].join("\n"), "utf8");
    await writeFile(
      path.join(tempRoot, "docs", "versioning-and-release-policy.md"),
      `- 代码版本为 \`${debOnlyFixtureVersion}\`\n`,
      "utf8",
    );
    await writeFile(debPath, "debian-beta", "utf8");
    await writeFile(`${debPath}.sig`, "deb-beta-signature\n", "utf8");

    await execFileAsync(process.execPath, [
      "--experimental-strip-types",
      releaseScriptPath,
      "prepare-linux-release-assets",
      debOnlyFixtureVersion,
      bundleDir,
      outputDir,
      "Asanilo/patina-Linux",
    ], { cwd: tempRoot });

    assert.equal(await readFile(path.join(outputDir, debName), "utf8"), "debian-beta");
    const outputEntries = await readdir(outputDir);
    assert.deepEqual(outputEntries.sort(), [debName, "latest.json"].sort());

    const latest = JSON.parse(await readFile(path.join(outputDir, "latest.json"), "utf8"));
    assert.deepEqual(latest.platforms, {
      "linux-x86_64-deb": {
        signature: "deb-beta-signature",
        url: `https://github.com/Asanilo/patina-Linux/releases/download/v${debOnlyFixtureVersion}/${debName}`,
      },
    });
  } finally {
    await rm(tempRoot, { force: true, recursive: true });
  }
}

async function testPrepareLinuxReleaseAssetsRejectsMissingDebSignature() {
  const tempRoot = await mkdtemp(path.join(tmpdir(), "patina-linux-release-missing-deb-signature-"));
  const bundleDir = path.join(tempRoot, "bundle");
  const outputDir = path.join(tempRoot, "output");
  const appImageName = `Patina_${currentPackageVersion}_amd64.AppImage`;
  const appImagePath = path.join(bundleDir, "appimage", appImageName);
  const debName = `Patina_${currentPackageVersion}_amd64.deb`;

  try {
    await mkdir(path.dirname(appImagePath), { recursive: true });
    await mkdir(path.join(bundleDir, "deb"), { recursive: true });
    await writeFile(appImagePath, "appimage", "utf8");
    await writeFile(`${appImagePath}.sig`, "appimage-signature\n", "utf8");
    await writeFile(path.join(bundleDir, "deb", debName), "debian", "utf8");

    let failure: unknown = null;
    try {
      await execFileAsync(process.execPath, [
        "--experimental-strip-types",
        "scripts/release.ts",
        "prepare-linux-release-assets",
        currentPackageVersion,
        bundleDir,
        outputDir,
        "Asanilo/patina-Linux",
      ]);
    } catch (error) {
      failure = error;
    }

    assert.ok(failure, "release preparation must reject a DEB without .deb.sig");
  } finally {
    await rm(tempRoot, { force: true, recursive: true });
  }
}

async function testPrepareLinuxReleaseAssetsRejectsEmptyDebSignature() {
  const tempRoot = await mkdtemp(path.join(tmpdir(), "patina-linux-release-empty-deb-signature-"));
  const bundleDir = path.join(tempRoot, "bundle");
  const outputDir = path.join(tempRoot, "output");
  const appImageName = `Patina_${currentPackageVersion}_amd64.AppImage`;
  const appImagePath = path.join(bundleDir, "appimage", appImageName);
  const debName = `Patina_${currentPackageVersion}_amd64.deb`;
  const debPath = path.join(bundleDir, "deb", debName);

  try {
    await mkdir(path.dirname(appImagePath), { recursive: true });
    await mkdir(path.dirname(debPath), { recursive: true });
    await writeFile(appImagePath, "appimage", "utf8");
    await writeFile(`${appImagePath}.sig`, "appimage-signature\n", "utf8");
    await writeFile(debPath, "debian", "utf8");
    await writeFile(`${debPath}.sig`, "\n", "utf8");

    let failure: unknown = null;
    try {
      await execFileAsync(process.execPath, [
        "--experimental-strip-types",
        "scripts/release.ts",
        "prepare-linux-release-assets",
        currentPackageVersion,
        bundleDir,
        outputDir,
        "Asanilo/patina-Linux",
      ]);
    } catch (error) {
      failure = error;
    }

    assert.ok(failure, "release preparation must reject an empty .deb.sig");
  } finally {
    await rm(tempRoot, { force: true, recursive: true });
  }
}

async function testReleaseAssetGuard() {
  const root = await mkdtemp(path.join(tmpdir(), "patina-release-guard-"));
  const version = "1.9.2";
  const files = [path.join(root, "Patina_1.9.2_amd64.AppImage"), path.join(root, "latest.json")];
  const contents = ["signed bundle fixture", '{"version":"1.9.2"}'];
  const assets = contents.map((content, index) => ({
    name: path.basename(files[index]), state: "uploaded", size: Buffer.byteLength(content),
    digest: `sha256:${createHash("sha256").update(content).digest("hex")}`,
  }));
  const release = { id: 17, tag_name: `v${version}`, prerelease: false, draft: false };
  const base = "https://api.github.com/repos/Asanilo/patina-Linux";
  const calls: string[] = [];
  function api(remoteRelease: object | null, pages: object[][] = [assets], failure = 0) {
    return async (url: string, options: RequestInit) => {
      calls.push(url);
      assert.equal(options.redirect, "error");
      assert.equal(options.method, undefined, "preflight must use read-only GET requests");
      if (failure) return new Response("failure", { status: failure });
      if (url === `${base}/releases/tags/v${version}`) {
        return remoteRelease
          ? Response.json(remoteRelease)
          : new Response("not found", { status: 404 });
      }
      const match = url.match(/\/releases\/17\/assets\?per_page=100&page=(\d+)$/);
      assert.ok(match, `unexpected API request: ${url}`);
      return Response.json(pages[Number(match[1]) - 1] ?? []);
    };
  }
  const guard = (request) => guardReleaseAssets(version, "Asanilo/patina-Linux", files, request);
  try {
    await Promise.all(files.map((file, index) => writeFile(file, contents[index])));
    assert.deepEqual(await guard(api(null)), { publish: true, files });
    assert.deepEqual(await guard(api(release)), { publish: false, files: [] });
    assert.deepEqual(await guard(api(release, [[assets[0]]])), { publish: true, files: [files[1]] });
    assert.deepEqual(await guard(api({ ...release, draft: true })), { publish: true, files: [] });

    for (const change of [
      { digest: `sha256:${"0".repeat(64)}` }, { digest: null },
      { size: assets[0].size + 1 }, { state: "starter" },
    ]) {
      await assert.rejects(guard(api(release, [[{ ...assets[0], ...change }, assets[1]]])),
        /differs or cannot be verified/);
    }
    // Never mix a retained package with a newly generated incompatible manifest.
    await writeFile(files[1], '{"version":"1.9.2","signature":"changed"}');
    await assert.rejects(guard(api(release)), /latest\.json; use a new version/);
    await writeFile(files[1], contents[1]);
    for (const status of [401, 403, 500]) {
      await assert.rejects(guard(api(null, [], status)), /release lookup failed/);
    }
    await assert.rejects(guard(async () => { throw new Error("offline"); }), /offline/);
    await assert.rejects(guard(api({ ...release, prerelease: true })), /identity or channel/);
    await assert.rejects(guard(api({ ...release, tag_name: "v1.9.1" })), /identity or channel/);
    await assert.rejects(guard(api({ ...release, immutable: true }, [[]])), /immutable release/);
    await assert.rejects(guard(api(release, [[assets[0], assets[0]]])), /duplicate existing/);
    const firstPage = Array.from({ length: 100 }, (_, index) => ({ name: `extra-${index}.txt` }));
    assert.deepEqual(await guard(api(release, [firstPage, assets])), { publish: false, files: [] });
    assert.ok(calls.includes(`${base}/releases/17/assets?per_page=100&page=2`));
    await assert.rejects(guardReleaseAssets(version, "Asanilo/patina-Linux", [files[0], files[0]], api(null)), /duplicate/);
    await writeFile(files[0], "");
    await assert.rejects(guard(api(null)), /invalid or duplicate release asset/);
  } finally {
    await rm(root, { recursive: true, force: true });
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
testDaemonBackedPrereleaseNotesOnlyOfferDebian();
testUpdaterPlatformsKeepStableAndPrereleaseContractsSeparate();
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
await testPrepareLinuxReleaseAssetsRejectsEmptyDebSignature();
await testPrepareLinuxReleaseAssetsRejectsMissingDebSignature();
await testPrepareStableLinuxReleaseAssetsCreatesBothPackageTargets();
await testPrepareStableLinuxReleaseAssetsCreatesBothPackageTargets("1.9.0-rc.1");
await testPreparePrereleaseLinuxAssetsCreatesOnlyDebianTarget();
await testReleaseAssetGuard();

await execFileAsync(process.execPath, ["--experimental-strip-types", releaseScriptPath, "validate-version-files"]);
await execFileAsync(process.execPath, ["--experimental-strip-types", releaseScriptPath, "validate-version-files", currentPackageVersion]);
await assert.rejects(
  execFileAsync(process.execPath, ["--experimental-strip-types", releaseScriptPath, "validate-version-files", "0.0.0-cli-mismatch"]),
  { code: 1 },
);

console.log("Passed 26 release policy tests");
