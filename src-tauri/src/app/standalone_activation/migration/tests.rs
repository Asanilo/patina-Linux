use super::*;
use std::fs;

struct Fixture {
    root: PathBuf,
    roots: AppPathRoots,
    paths: StoragePaths,
    target: String,
    proof: MigrationProof,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "patina-migration-{}",
            super::super::journal::random_id().unwrap()
        ));
        let roots = AppPathRoots {
            config: root.join("config"),
            data: root.join("data"),
            local_data: root.join("data"),
        };
        let paths = crate::platform::storage_paths::default_storage_paths_for_profile(
            &roots,
            AppProfile::Production,
        );
        let original = unit::appimage(
            &paths
                .stable_product_data_root
                .join("runtime-appimage/current/AppRun"),
            &roots.config,
            &roots.data,
        )
        .unwrap();
        let target = unit::standalone(&root.join("runtime"), &roots.config, &roots.data).unwrap();
        let proof = MigrationProof {
            request: MigrationRequest::parse("appimage", &unit_hash(&original), "1.9.2").unwrap(),
            original_unit: original,
            source_unit_path: roots.config.join("systemd/user/patinad.service"),
            image_sha256: Some("a".repeat(64)),
        };
        Self {
            root,
            roots,
            paths,
            target,
            proof,
        }
    }
    fn saved(&self) -> ActivationRecord {
        ActivationRecord {
            format_version: 1,
            runtime_root: self.root.join("runtime"),
            config_root: self.roots.config.clone(),
            data_root: self.roots.data.clone(),
            manifest_sha256: "a".repeat(64),
            binary_sha256: "b".repeat(64),
            cutover_request_id: format!("cutover_{}", "a".repeat(32)),
            phase: Phase::Prepared,
            runtime_start_allowed: true,
            minimum_runtime_version: None,
            deactivation_mask: None,
            deactivation_binary_sha256: None,
            last_error: None,
            migration: Some(self.proof.clone()),
        }
    }
    fn open(
        &self,
        request: MigrationRequest,
        version: &str,
        saved: Option<&ActivationRecord>,
    ) -> Result<LegacySource, String> {
        LegacySource::open(
            &self.roots,
            &self.paths,
            request,
            version,
            &self.target,
            saved,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn confirmation_rejects_unknown_sources_and_downgrades_before_source_access() {
    assert!(MigrationRequest::parse("custom", &"a".repeat(64), "1.9.2").is_err());
    assert!(MigrationRequest::parse("packaged", &"A".repeat(64), "1.9.2").is_err());
    assert!(MigrationRequest::parse("packaged", &"a".repeat(64), "unknown").is_err());
    let fixture = Fixture::new();
    let error = fixture
        .open(fixture.proof.request.clone(), "1.9.1", None)
        .err()
        .unwrap();
    assert!(error.contains("older"));
    assert!(!fixture.root.exists());
    let mut wrong = fixture.proof.request.clone();
    wrong.unit_sha256 = "0".repeat(64);
    assert!(fixture
        .open(wrong, "1.9.2", None)
        .err()
        .unwrap()
        .contains("known service recipe"));
    unit::install_identical_or_new(&fixture.roots.config, "custom unit").unwrap();
    assert!(fixture
        .open(fixture.proof.request.clone(), "1.9.2", None)
        .err()
        .unwrap()
        .contains("preserved"));
    assert_eq!(
        unit::read_existing(&fixture.proof.source_unit_path)
            .unwrap()
            .unwrap(),
        "custom unit"
    );
    assert!(!fixture.paths.stable_product_data_root.exists());
}

#[tokio::test]
async fn published_target_resumes_only_with_matching_durable_source_proof() {
    let fixture = Fixture::new();
    unit::install_identical_or_new(&fixture.roots.config, &fixture.target).unwrap();
    assert!(fixture
        .open(fixture.proof.request.clone(), "1.9.2", None)
        .is_err());
    let saved = fixture.saved();
    let source = fixture
        .open(
            fixture.proof.request.clone(),
            "1.9.2+new-build",
            Some(&saved),
        )
        .unwrap();
    // Old AppImage files may already have been removed after publication. Recovery
    // verifies loaded ownership separately, never executes or recreates old files.
    source.verify_files().await.unwrap();
    assert!(source.replaced && source.appimage.is_none());
    assert!(!fixture.paths.stable_product_data_root.exists());
    let mut wrong = fixture.proof.request.clone();
    wrong.source_version = "1.9.1".into();
    assert!(fixture
        .open(wrong, "1.9.2", Some(&saved))
        .err()
        .unwrap()
        .contains("saved intent"));
    let mut invalid = saved;
    invalid
        .migration
        .as_mut()
        .unwrap()
        .original_unit
        .push_str("changed");
    assert!(fixture
        .open(fixture.proof.request.clone(), "1.9.2", Some(&invalid))
        .is_err());
}

#[test]
fn loaded_contract_checks_argv_environment_and_environment_files() {
    let fixture = Fixture::new();
    unit::install_identical_or_new(&fixture.roots.config, &fixture.target).unwrap();
    let source = fixture
        .open(
            fixture.proof.request.clone(),
            "1.9.2",
            Some(&fixture.saved()),
        )
        .unwrap();
    let launch = || systemd::ServiceLaunch {
        main_pid: 42,
        commands: vec![(
            source.launcher.to_str().unwrap().into(),
            source.arguments.clone(),
            false,
        )],
        environment: source.environment.clone(),
        environment_files: vec![],
    };
    let mut valid = launch();
    valid.environment.reverse();
    source.verify_launch(&valid).unwrap();
    let mut changed = launch();
    changed.commands[0].1.push("--api-port=1".into());
    assert!(source.verify_launch(&changed).is_err());
    let mut changed = launch();
    changed.commands[0].2 = true;
    assert!(source.verify_launch(&changed).is_err());
    let mut changed = launch();
    changed
        .environment
        .push("PRIVATE_SECRET=never-print-me".into());
    let error = source.verify_launch(&changed).unwrap_err();
    assert!(!error.contains("never-print-me"));
    let mut changed = launch();
    changed.environment_files.push(("/env".into(), false));
    assert!(source.verify_launch(&changed).is_err());
}
