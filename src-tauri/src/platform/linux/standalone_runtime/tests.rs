use super::*;
use std::os::unix::fs::symlink;

struct Fixture {
    _temporary: TemporaryDirectory,
    source: PathBuf,
    root: PathBuf,
    manifest: serde_json::Value,
}

impl Fixture {
    fn new() -> Self {
        let temporary =
            TemporaryDirectory::create(&std::env::temp_dir(), "patina-stage-test").unwrap();
        let source = temporary.path().join("source");
        private_directory(&source).unwrap();
        private_directory(&source.join("bin")).unwrap();
        private_directory(&source.join("systemd")).unwrap();
        let mut files = serde_json::Map::new();
        for (name, mode, _) in PAYLOADS {
            let content = if name == "bin/patinad" {
                b"\x7fELFfixture-never-executed".as_slice()
            } else {
                b"fixture".as_slice()
            };
            write_new(&source.join(name), content, mode).unwrap();
            fs::set_permissions(source.join(name), fs::Permissions::from_mode(mode)).unwrap();
            files.insert(
                name.into(),
                serde_json::json!({"sha256":digest(content),"size":content.len(),"mode":mode}),
            );
        }
        let mut build = crate::app::daemon::build_info::current();
        build.desktop_feature = false;
        build.debug_assertions = false;
        let manifest = serde_json::json!({"format_version":1,"distribution":"standalone","build":build,"files":files});
        let root = temporary.path().join("installed");
        Self {
            _temporary: temporary,
            source,
            root,
            manifest,
        }
    }
    fn save_manifest(&self) -> String {
        let bytes = serde_json::to_vec_pretty(&self.manifest).unwrap();
        fs::write(self.source.join("manifest.json"), &bytes).unwrap();
        digest(&bytes)
    }
}

#[test]
fn staging_binds_identity_reuses_verified_versions_and_does_not_activate() {
    let fixture = Fixture::new();
    let expected = fixture.save_manifest();
    let first = stage(&fixture.source, &fixture.root, &expected, false).unwrap();
    assert_eq!(
        first.directory,
        fixture.root.join("versions").join(&expected)
    );
    assert_eq!(first.manifest_sha256, expected);
    assert_eq!(
        first.binary_sha256,
        fixture.manifest["files"]["bin/patinad"]["sha256"]
    );
    assert_eq!(
        fs::read(first.directory.join("bin/patinad")).unwrap(),
        b"\x7fELFfixture-never-executed"
    );
    assert_eq!(fs::metadata(&fixture.root).unwrap().mode() & 0o777, 0o700);
    assert!(!fixture.root.join("current").exists());
    assert!(!fixture.root.join("patinad.service").exists());
    let second = stage(&fixture.source, &fixture.root, &expected, false).unwrap();
    assert_eq!(first.directory, second.directory);
    assert_eq!(
        fs::read_dir(fixture.root.join("versions")).unwrap().count(),
        1
    );
}

#[test]
fn invalid_source_never_initializes_the_installation_root() {
    let fixture = Fixture::new();
    let expected = fixture.save_manifest();
    assert!(stage(&fixture.source, &fixture.root, &"a".repeat(64), false).is_err());
    fs::write(fixture.source.join("LICENSE"), b"modified").unwrap();
    assert!(stage(&fixture.source, &fixture.root, &expected, false).is_err());
    assert!(!fixture.root.exists());
}

#[test]
fn damaged_installed_versions_are_not_overwritten_or_repaired() {
    let fixture = Fixture::new();
    let expected = fixture.save_manifest();
    let installed = stage(&fixture.source, &fixture.root, &expected, false).unwrap();
    fs::write(installed.directory.join("LICENSE"), b"modified").unwrap();
    assert!(stage(&fixture.source, &fixture.root, &expected, false).is_err());
    assert_eq!(
        fs::read(installed.directory.join("LICENSE")).unwrap(),
        b"modified"
    );
    assert_eq!(
        fs::read_dir(fixture.root.join("versions")).unwrap().count(),
        1
    );
}

#[test]
fn source_links_extra_files_and_privileged_modes_are_rejected() {
    for kind in ["symlink", "parent-link", "hardlink", "extra", "setuid"] {
        let fixture = Fixture::new();
        let expected = fixture.save_manifest();
        let source_file = fixture.source.join("bin/patinad");
        match kind {
            "symlink" => {
                fs::remove_file(&source_file).unwrap();
                symlink(fixture.source.join("LICENSE"), &source_file).unwrap();
            }
            "parent-link" => {
                let external = fixture._temporary.path().join("external-bin");
                fs::rename(fixture.source.join("bin"), &external).unwrap();
                symlink(external, fixture.source.join("bin")).unwrap();
            }
            "hardlink" => {
                fs::hard_link(
                    &source_file,
                    fixture._temporary.path().join("linked-binary"),
                )
                .unwrap();
            }
            "extra" => {
                fs::write(fixture.source.join("unexpected"), b"extra").unwrap();
            }
            "setuid" => {
                fs::set_permissions(&source_file, fs::Permissions::from_mode(0o4755)).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            stage(&fixture.source, &fixture.root, &expected, false).is_err(),
            "{kind}"
        );
        assert!(!fixture.root.exists(), "{kind}");
    }
}

#[test]
fn metadata_projection_target_bounds_and_file_set_are_validated() {
    for (pointer, value) in [
        ("/format_version", serde_json::json!(2)),
        ("/distribution", serde_json::json!("other")),
        ("/build/desktop_feature", serde_json::json!(true)),
        ("/build/debug_assertions", serde_json::json!(true)),
        ("/build/target", serde_json::json!("different-target")),
        ("/build/package_version", serde_json::json!("invalid")),
        (
            "/build/protocol/min_supported_client",
            serde_json::json!(99),
        ),
        ("/files/LICENSE/size", serde_json::json!(u64::MAX)),
        ("/files/LICENSE/mode", serde_json::json!(0o666)),
    ] {
        let mut fixture = Fixture::new();
        *fixture.manifest.pointer_mut(pointer).unwrap() = value;
        let expected = fixture.save_manifest();
        assert!(
            stage(&fixture.source, &fixture.root, &expected, false).is_err(),
            "{pointer}"
        );
        assert!(!fixture.root.exists());
    }
    let mut fixture = Fixture::new();
    let license = fixture.manifest["files"]
        .as_object_mut()
        .unwrap()
        .remove("LICENSE")
        .unwrap();
    fixture.manifest["files"]["../../escape"] = license;
    assert!(stage(
        &fixture.source,
        &fixture.root,
        &fixture.save_manifest(),
        false
    )
    .is_err());
    assert!(!fixture.root.exists());
}

#[test]
fn explicit_debug_staging_is_allowed_without_changing_the_default() {
    let mut fixture = Fixture::new();
    fixture.manifest["build"]["debug_assertions"] = serde_json::json!(true);
    let expected = fixture.save_manifest();
    assert!(stage(&fixture.source, &fixture.root, &expected, false).is_err());
    assert!(
        stage(&fixture.source, &fixture.root, &expected, true)
            .unwrap()
            .build
            .debug_assertions
    );
}

#[test]
fn unmanaged_roots_links_and_busy_installations_are_preserved() {
    let fixture = Fixture::new();
    let expected = fixture.save_manifest();
    private_directory(&fixture.root).unwrap();
    fs::write(fixture.root.join("user-data"), b"untouched").unwrap();
    assert!(stage(&fixture.source, &fixture.root, &expected, false).is_err());
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 1);
    assert_eq!(
        fs::read(fixture.root.join("user-data")).unwrap(),
        b"untouched"
    );
    let linked = fixture._temporary.path().join("linked-root");
    symlink(&fixture.root, &linked).unwrap();
    assert!(stage(&fixture.source, &linked, &expected, false).is_err());
    fs::remove_file(fixture.root.join("user-data")).unwrap();
    stage(&fixture.source, &fixture.root, &expected, false).unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(fixture.root.join("install.lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(stage(&fixture.source, &fixture.root, &expected, false)
        .unwrap_err()
        .contains("in progress"));
    drop(lock);
    assert!(stage(&fixture.source, &fixture.root, &expected, false).is_ok());
}
