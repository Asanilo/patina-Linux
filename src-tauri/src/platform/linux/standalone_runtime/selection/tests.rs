use super::super::tests::Fixture;
use super::*;

fn staged(fixture: &mut Fixture, version: &str) -> String {
    fixture.manifest["build"]["package_version"] = serde_json::json!(version);
    let manifest = fixture.save_manifest();
    stage(&fixture.source, &fixture.root, &manifest, false).unwrap();
    manifest
}

fn selected(root: &Path) -> StagedRuntime {
    inspect(root, false).unwrap().selected.unwrap()
}

#[test]
fn service_preview_requires_current_identity_and_preserves_existing_user_unit() {
    let mut fixture = Fixture::new();
    let manifest = staged(&mut fixture, "1.0.0");
    let config = fixture.source.join("config");
    let data = fixture.source.join("data");
    assert!(service_plan(&fixture.root, &manifest, &config, &data, false).is_err());
    select(&fixture.root, &manifest, &ExpectedCurrent::Absent, false).unwrap();
    let plan = service_plan(&fixture.root, &manifest, &config, &data, false).unwrap();
    assert!(!config.exists());
    assert!(!data.exists());
    assert_eq!(plan.selected.manifest_sha256, manifest);
    assert_eq!(plan.unit_path, config.join("systemd/user/patinad.service"));
    fs::create_dir_all(plan.unit_path.parent().unwrap()).unwrap();
    fs::write(&plan.unit_path, b"custom unit").unwrap();
    assert_eq!(
        service_plan(&fixture.root, &manifest, &config, &data, false)
            .unwrap()
            .unit_text,
        plan.unit_text
    );
    assert_eq!(fs::read(&plan.unit_path).unwrap(), b"custom unit");
    assert!(service_plan(&fixture.root, &"f".repeat(64), &config, &data, false).is_err());
    assert!(service_plan(
        &fixture.root,
        &manifest,
        Path::new("relative"),
        &data,
        false
    )
    .is_err());
}

#[test]
fn selection_exposes_verified_identity_and_repeating_same_baseline_is_idempotent() {
    let mut fixture = Fixture::new();
    let first = staged(&mut fixture, "1.0.0");
    assert!(inspect(&fixture.root, false).unwrap().selected.is_none());
    let chosen = select(&fixture.root, &first, &ExpectedCurrent::Absent, false).unwrap();
    assert_eq!(selected(&fixture.root).binary_sha256, chosen.binary_sha256);
    assert_eq!(
        fs::read_link(fixture.root.join("current")).unwrap(),
        Path::new("versions").join(&first)
    );
    let before = fs::symlink_metadata(fixture.root.join("current"))
        .unwrap()
        .ino();
    select(
        &fixture.root,
        &first,
        &ExpectedCurrent::Manifest(first.clone()),
        false,
    )
    .unwrap();
    assert_eq!(
        before,
        fs::symlink_metadata(fixture.root.join("current"))
            .unwrap()
            .ino()
    );
    assert!(select(&fixture.root, &first, &ExpectedCurrent::Absent, false).is_err());
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 4); // marker, lock, versions, current
}

#[test]
fn competing_selectors_cannot_both_commit_from_the_same_baseline() {
    let mut fixture = Fixture::new();
    let first = staged(&mut fixture, "1.0.0");
    let second = staged(&mut fixture, "2.0.0");
    let third = staged(&mut fixture, "3.0.0");
    select(&fixture.root, &first, &ExpectedCurrent::Absent, false).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let outcomes = std::thread::scope(|scope| {
        let workers = [&second, &third]
            .into_iter()
            .map(|target| {
                let barrier = barrier.clone();
                let root = &fixture.root;
                let first = &first;
                scope.spawn(move || {
                    barrier.wait();
                    select(
                        root,
                        target,
                        &ExpectedCurrent::Manifest(first.clone()),
                        false,
                    )
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    let winner = outcomes.into_iter().find_map(Result::ok).unwrap();
    assert_eq!(
        selected(&fixture.root).manifest_sha256,
        winner.manifest_sha256
    );
    assert!(select(
        &fixture.root,
        &second,
        &ExpectedCurrent::Manifest(first),
        false
    )
    .is_err());
}

#[test]
fn downgrade_is_rejected_but_same_version_different_build_identity_is_explicit() {
    let mut fixture = Fixture::new();
    let first = staged(&mut fixture, "1.0.0");
    let second = staged(&mut fixture, "2.0.0+first");
    let third = staged(&mut fixture, "2.0.0+second");
    select(&fixture.root, &second, &ExpectedCurrent::Absent, false).unwrap();
    assert!(select(
        &fixture.root,
        &first,
        &ExpectedCurrent::Manifest(second.clone()),
        false
    )
    .is_err());
    assert_eq!(selected(&fixture.root).manifest_sha256, second);
    select(
        &fixture.root,
        &third,
        &ExpectedCurrent::Manifest(second),
        false,
    )
    .unwrap();
    assert_eq!(selected(&fixture.root).manifest_sha256, third);
}

#[test]
fn invalid_current_paths_and_custom_content_are_preserved() {
    for kind in [
        "file",
        "directory",
        "absolute",
        "traversal",
        "double-slash",
        "missing",
    ] {
        let mut fixture = Fixture::new();
        let manifest = staged(&mut fixture, "1.0.0");
        let current = fixture.root.join("current");
        match kind {
            "file" => fs::write(&current, b"user contents").unwrap(),
            "directory" => fs::create_dir(&current).unwrap(),
            "absolute" => symlink(fixture.root.join("versions").join(&manifest), &current).unwrap(),
            "traversal" => symlink(format!("versions/../versions/{manifest}"), &current).unwrap(),
            "double-slash" => symlink(format!("versions//{manifest}"), &current).unwrap(),
            "missing" => symlink(format!("versions/{}", "f".repeat(64)), &current).unwrap(),
            _ => unreachable!(),
        }
        let inode = fs::symlink_metadata(&current).unwrap().ino();
        assert!(inspect(&fixture.root, false).is_err(), "{kind}");
        assert!(
            select(&fixture.root, &manifest, &ExpectedCurrent::Absent, false).is_err(),
            "{kind}"
        );
        assert_eq!(fs::symlink_metadata(&current).unwrap().ino(), inode);
        if kind == "file" {
            assert_eq!(fs::read(&current).unwrap(), b"user contents");
        }
    }
}

#[test]
fn missing_or_damaged_versions_never_replace_selection_or_repair_payloads() {
    let mut fixture = Fixture::new();
    let first = staged(&mut fixture, "1.0.0");
    let second = staged(&mut fixture, "2.0.0");
    select(&fixture.root, &first, &ExpectedCurrent::Absent, false).unwrap();
    let changed = fixture.root.join("versions").join(&second).join("LICENSE");
    fs::write(&changed, b"tampered").unwrap();
    for target in [&second, &"f".repeat(64), "../other"] {
        assert!(select(
            &fixture.root,
            target,
            &ExpectedCurrent::Manifest(first.clone()),
            false
        )
        .is_err());
        assert_eq!(selected(&fixture.root).manifest_sha256, first);
    }
    assert_eq!(fs::read(changed).unwrap(), b"tampered");
    let current_file = fixture.root.join("versions").join(&first).join("LICENSE");
    fs::write(&current_file, b"also tampered").unwrap();
    assert!(inspect(&fixture.root, false).is_err());
    assert!(select(
        &fixture.root,
        &second,
        &ExpectedCurrent::Manifest(first.clone()),
        false
    )
    .is_err());
    assert_eq!(
        fs::read_link(fixture.root.join("current")).unwrap(),
        Path::new("versions").join(first)
    );
}

#[test]
fn inspection_never_initializes_roots_and_all_installation_operations_share_the_lock() {
    let mut fixture = Fixture::new();
    assert!(inspect(&fixture.root, false).is_err());
    assert!(!fixture.root.exists());
    let manifest = staged(&mut fixture, "1.0.0");
    let lock = installation_lock(&fixture.root, false, true).unwrap();
    assert!(inspect(&fixture.root, false)
        .unwrap_err()
        .contains("in progress"));
    assert!(
        select(&fixture.root, &manifest, &ExpectedCurrent::Absent, false)
            .unwrap_err()
            .contains("in progress")
    );
    drop(lock);
    let reader = installation_lock(&fixture.root, false, false).unwrap();
    assert!(inspect(&fixture.root, false).is_ok());
    assert!(select(&fixture.root, &manifest, &ExpectedCurrent::Absent, false).is_err());
    assert!(stage(&fixture.source, &fixture.root, &manifest, false).is_err());
    drop(reader);
    select(&fixture.root, &manifest, &ExpectedCurrent::Absent, false).unwrap();
}

#[test]
fn debug_selection_requires_explicit_opt_in_and_expected_identity_is_strict() {
    let mut fixture = Fixture::new();
    fixture.manifest["build"]["debug_assertions"] = serde_json::json!(true);
    let manifest = fixture.save_manifest();
    stage(&fixture.source, &fixture.root, &manifest, true).unwrap();
    assert!(select(&fixture.root, &manifest, &ExpectedCurrent::Absent, false).is_err());
    assert!(!fixture.root.join("current").exists());
    select(&fixture.root, &manifest, &ExpectedCurrent::Absent, true).unwrap();
    assert!(inspect(&fixture.root, false).is_err());
    assert!(
        inspect(&fixture.root, true)
            .unwrap()
            .selected
            .unwrap()
            .build
            .debug_assertions
    );
    assert_eq!(
        ExpectedCurrent::parse("none").unwrap(),
        ExpectedCurrent::Absent
    );
    assert!(ExpectedCurrent::parse("NONE").is_err());
    assert!(ExpectedCurrent::parse("../version").is_err());
}

#[test]
fn installation_guard_releases_ownership_even_if_a_duplicate_descriptor_survives() {
    let mut fixture = Fixture::new();
    staged(&mut fixture, "1.0.0");
    let raw = OpenOptions::new()
        .read(true)
        .write(true)
        .open(fixture.root.join("install.lock"))
        .unwrap();
    raw.try_lock_exclusive().unwrap();
    let duplicate = raw.try_clone().unwrap();
    drop(raw);
    assert!(
        inspect(&fixture.root, false).is_err(),
        "closing one duplicate alone retains flock ownership"
    );
    FileExt::unlock(&duplicate).unwrap();
    drop(duplicate);
    let guard = installation_lock(&fixture.root, false, true).unwrap();
    let inherited = guard.0.try_clone().unwrap();
    assert!(inspect(&fixture.root, false).is_err());
    drop(guard);
    assert!(inspect(&fixture.root, false).is_ok());
    drop(inherited);
}
