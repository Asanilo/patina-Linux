//! Same-filesystem replacement of an already signature-verified AppImage.
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const MAX_IMAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn fail(error: impl std::fmt::Display) -> String {
    format!("AppImage atomic installation failed: {error}")
}

fn regular_owned(path: &Path) -> Result<fs::Metadata, String> {
    let meta = fs::symlink_metadata(path).map_err(fail)?;
    // SAFETY: geteuid has no pointer arguments or side effects.
    if !meta.is_file() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o6022 != 0 {
        return Err(fail(
            "expected a user-owned regular file without group/world write or special permissions",
        ));
    }
    if meta.len() > MAX_IMAGE_BYTES {
        return Err(fail("image exceeds size budget"));
    }
    Ok(meta)
}

pub(crate) fn fingerprint(path: &Path) -> Result<String, String> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(fail)?;
    let metadata = file.metadata().map_err(fail)?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
        return Err(fail("expected a bounded regular AppImage file"));
    }
    let mut hash = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    let mut total = 0u64;
    loop {
        let size = file.read(&mut bytes).map_err(fail)?;
        if size == 0 {
            break;
        }
        if total == 0
            && (bytes.get(..4) != Some(b"\x7fELF")
                || bytes.get(8..11) != Some(b"AI\x02")
                || size < 11)
        {
            return Err(fail("existing file is not a type-2 AppImage"));
        }
        total += size as u64;
        if total > MAX_IMAGE_BYTES {
            return Err(fail("image exceeds size budget"));
        }
        hash.update(&bytes[..size]);
    }
    if total < 11 {
        return Err(fail("existing file is not a type-2 AppImage"));
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// `bytes` must come directly from the successful Tauri Update::download result,
/// which verifies the updater signature before exposing the buffer to Rust.
pub(crate) fn install_verified_image(target: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES
        || bytes.get(..4) != Some(b"\x7fELF")
        || bytes.get(8..11) != Some(b"AI\x02")
    {
        return Err(fail("expected a raw type-2 AppImage, not a DEB or archive"));
    }
    if !target.is_absolute() {
        return Err(fail("image path must be absolute"));
    }
    let meta = regular_owned(target)?;
    let parent = target
        .parent()
        .ok_or_else(|| fail("missing image parent"))?
        .canonicalize()
        .map_err(fail)?;
    let parent_meta = fs::metadata(&parent).map_err(fail)?;
    if parent_meta.mode() & 0o022 != 0 {
        return Err(fail("image parent is writable by other users"));
    }
    let target = parent.join(target.file_name().ok_or_else(|| fail("missing filename"))?);
    let previous_hash = fingerprint(&target)?;
    let previous = parent.join(format!(".patina-previous-{previous_hash}.AppImage"));
    if format!("{:x}", Sha256::digest(bytes)) == previous_hash {
        return Ok(target);
    }
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(fail)?;
    let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let stage_path = parent.join(format!(".patina-update-{suffix}.tmp"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&stage_path)
        .map_err(fail)?;
    let stage = Stage(stage_path);
    file.write_all(bytes).map_err(fail)?;
    file.set_permissions(fs::Permissions::from_mode(meta.mode() & 0o777))
        .map_err(fail)?;
    file.sync_all().map_err(fail)?;
    let current = regular_owned(&target)?;
    if current.dev() != meta.dev()
        || current.ino() != meta.ino()
        || fingerprint(&target)? != previous_hash
    {
        return Err(fail(
            "installed image changed during staging; retry after checking the package",
        ));
    }
    match fs::hard_link(&target, &previous) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            regular_owned(&previous)?;
            if fingerprint(&previous)? != previous_hash {
                return Err(fail("recovery image conflicts with an existing file"));
            }
        }
        Err(error) => return Err(fail(error)),
    }
    let directory = File::open(&parent).map_err(fail)?;
    directory.sync_all().map_err(fail)?;
    fs::rename(&stage.0, &target).map_err(fail)?;
    // A failure here may mean the rename already occurred. The complete new file
    // and old recovery image remain; retry is idempotent, not a partial rewrite.
    directory.sync_all().map_err(fail)?;
    Ok(previous)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires isolated real AppImages and a production-signed candidate, never a private key"]
    async fn production_signed_appimage_upgrade() {
        use tauri_plugin_updater::UpdaterExt;
        let root = PathBuf::from(std::env::var_os("PATINA_SIGNED_UPGRADE_TEST_ROOT").unwrap());
        assert!(root.is_absolute());
        assert_eq!(fs::canonicalize(&root).unwrap(), root);
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::read_to_string(root.join("marker")).unwrap(),
            "isolated-signed-upgrade\n"
        );
        let target = root.join("installed.AppImage");
        let old_hash = fingerprint(&target).unwrap();
        let input: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("input.json")).unwrap()).unwrap();
        let old_version = semver::Version::parse(input["old_version"].as_str().unwrap()).unwrap();
        let new_version = semver::Version::parse(input["new_version"].as_str().unwrap()).unwrap();
        assert!(new_version > old_version);
        assert_eq!(input["old_sha256"].as_str().unwrap(), old_hash);
        let bytes = fs::read(root.join("candidate.AppImage")).unwrap();
        let new_hash = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(input["new_sha256"].as_str().unwrap(), new_hash);
        let signature = fs::read_to_string(root.join("candidate.AppImage.sig")).unwrap();
        // The trust anchor comes from the application config, never from the
        // candidate directory or a test-generated key.
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../../tauri.conf.json")).unwrap();
        let public_key = config["plugins"]["updater"]["pubkey"].as_str().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let manifest = serde_json::json!({
            "version": new_version.to_string(), "url": format!("http://{address}/image"),
            "signature": signature.trim(),
        });
        let payload = bytes.clone();
        let mut tampered = bytes.clone();
        *tampered.last_mut().unwrap() ^= 1;
        let router = axum::Router::new()
            .route(
                "/latest",
                axum::routing::get(move || async move { axum::Json(manifest) }),
            )
            .route("/image", axum::routing::get(move || async move { payload }))
            .route(
                "/tampered",
                axum::routing::get(move || async move { tampered }),
            );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.package_info_mut().version = old_version;
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({
                "dangerousInsecureTransportProtocol": true, "pubkey": public_key,
            }),
        );
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap();
        let updater = app
            .updater_builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(60))
            .endpoints(vec![format!("http://{address}/latest").parse().unwrap()])
            .unwrap()
            .build()
            .unwrap();
        let mut update = updater.check().await.unwrap().unwrap();
        let download_url = update.download_url.clone();
        update.download_url = format!("http://{address}/tampered").parse().unwrap();
        assert!(update.download(|_, _| {}, || {}).await.is_err());
        assert_eq!(fingerprint(&target).unwrap(), old_hash);
        update.download_url = download_url;
        let verified = update.download(|_, _| {}, || {}).await.unwrap();
        assert_eq!(verified, bytes);
        let previous = install_verified_image(&target, &verified).unwrap();
        assert_eq!(fingerprint(&previous).unwrap(), old_hash);
        assert_eq!(fingerprint(&target).unwrap(), new_hash);
        fs::write(root.join("verified-install.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "production_key_verified": true, "tampered_rejected_before_install": true,
            "old_sha256": old_hash, "new_sha256": new_hash, "previous": previous,
            "scope": "Tauri download and atomic install over isolated loopback; not public update-channel delivery",
        })).unwrap()).unwrap();
        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    #[ignore = "requires an isolated signed fixture and loopback networking; never a release key"]
    async fn tauri_download_verifies_before_atomic_install() {
        use tauri_plugin_updater::UpdaterExt;
        let root = PathBuf::from(std::env::var_os("PATINA_UPDATER_TEST_ROOT").unwrap());
        let bytes = fs::read(root.join("fixture.AppImage")).unwrap();
        let key = fs::read_to_string(root.join("test.key.pub")).unwrap();
        let signature = fs::read_to_string(root.join("fixture.AppImage.sig")).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let manifest = serde_json::json!({
            "version": "99.0.0", "url": format!("http://{address}/image"),
            "signature": signature.trim(),
        });
        let payload = bytes.clone();
        let mut tampered = bytes.clone();
        *tampered.last_mut().unwrap() ^= 1;
        let router = axum::Router::new()
            .route(
                "/latest",
                axum::routing::get(move || async move { axum::Json(manifest) }),
            )
            .route("/image", axum::routing::get(move || async move { payload }))
            .route(
                "/tampered",
                axum::routing::get(move || async move { tampered }),
            );
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({
                "dangerousInsecureTransportProtocol": true,
                "pubkey": key.trim(),
            }),
        );
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap();
        let updater = app
            .updater_builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(15))
            .endpoints(vec![format!("http://{address}/latest").parse().unwrap()])
            .unwrap()
            .build()
            .unwrap();
        let mut update = updater.check().await.unwrap().unwrap();
        let verified = update.download(|_, _| {}, || {}).await.unwrap();
        assert_eq!(verified, bytes);
        let target = root.join("installed.AppImage");
        let old = image(1);
        fs::write(&target, &old).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
        let previous = install_verified_image(&target, &verified).unwrap();
        assert_eq!(fs::read(&previous).unwrap(), old);
        update.download_url = format!("http://{address}/tampered").parse().unwrap();
        assert!(update.download(|_, _| {}, || {}).await.is_err());
        assert_eq!(fs::read(&target).unwrap(), bytes);
        update.signature = "invalid-signature".into();
        assert!(update.download(|_, _| {}, || {}).await.is_err());
        assert_eq!(fs::read(&target).unwrap(), bytes);
        server.abort();
        let _ = server.await;
    }

    fn image(value: u8) -> Vec<u8> {
        let mut bytes = vec![value; 128];
        bytes[..4].copy_from_slice(b"\x7fELF");
        bytes[8..11].copy_from_slice(b"AI\x02");
        bytes
    }

    #[test]
    fn replacement_is_complete_and_retains_old_image_without_deleting_other_files() {
        let root = std::env::temp_dir().join(format!(
            "patina-image-test-{}-{}",
            std::process::id(),
            crate::app::runtime::now_ms()
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let target = root.join("Patina.AppImage");
        let old = image(1);
        let new = image(2);
        fs::write(&target, &old).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root.join("unrelated"), b"keep").unwrap();
        assert!(install_verified_image(&target, b"!<arch>\nDEB").is_err());
        assert_eq!(fs::read(&target).unwrap(), old);
        let collision = root.join(format!(
            ".patina-previous-{:x}.AppImage",
            Sha256::digest(&old)
        ));
        fs::write(&collision, b"unrelated recovery file").unwrap();
        assert!(install_verified_image(&target, &new).is_err());
        assert_eq!(fs::read(&target).unwrap(), old);
        assert_eq!(fs::read(&collision).unwrap(), b"unrelated recovery file");
        assert!(!fs::read_dir(&root).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".patina-update-")));
        fs::remove_file(&collision).unwrap();
        let recovery = install_verified_image(&target, &new).unwrap();
        assert_eq!(fs::read(&target).unwrap(), new);
        assert_eq!(fs::read(&recovery).unwrap(), old);
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let files = fs::read_dir(&root).unwrap().count();
        install_verified_image(&target, &new).unwrap();
        assert_eq!(fs::read_dir(&root).unwrap().count(), files);
        let link = root.join("link.AppImage");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(install_verified_image(&link, &old).is_err());
        assert_eq!(fs::read(root.join("unrelated")).unwrap(), b"keep");
        let empty = root.join("empty");
        fs::write(&empty, []).unwrap();
        assert!(fingerprint(&empty).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
