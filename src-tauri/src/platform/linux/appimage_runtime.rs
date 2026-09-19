//! Durable AppDir staging. Never use a temporary FUSE mount in a systemd unit.
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{symlink, DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

const MARKER: &str = ".patina-runtime.json";
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_FILES: usize = 30_000;

#[derive(Debug, Serialize, Deserialize)]
struct Identity {
    format: u32,
    version: String,
    image_sha256: String,
}

#[derive(Debug)]
pub(crate) struct PreparedRuntime {
    root: PathBuf,
    stage: Option<PathBuf>,
    destination: PathBuf,
    identity: Identity,
    _lock: File,
}

fn fail(error: impl std::fmt::Display) -> String {
    format!("AppImage runtime: {error}")
}

fn private_directory(path: &Path) -> Result<(), String> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(fail(error)),
    }
    let meta = fs::symlink_metadata(path).map_err(fail)?;
    // SAFETY: geteuid has no pointer arguments.
    if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
        return Err(fail(
            "runtime directory must be a private user-owned directory, not a link",
        ));
    }
    Ok(())
}

fn identity(path: &Path) -> Result<Identity, String> {
    let file = path.join(MARKER);
    let meta = fs::symlink_metadata(&file).map_err(fail)?;
    if !meta.is_file() || meta.len() > 4096 {
        return Err(fail("invalid runtime identity"));
    }
    let value: Identity = serde_json::from_slice(&fs::read(file).map_err(fail)?).map_err(fail)?;
    if value.format != 1
        || value.image_sha256.len() != 64
        || !value.image_sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(fail("invalid runtime identity"));
    }
    semver::Version::parse(&value.version).map_err(fail)?;
    Ok(value)
}

impl PreparedRuntime {
    pub(crate) fn prepare(
        source: &Path,
        root: &Path,
        version: &str,
        image_sha256: &str,
    ) -> Result<Self, String> {
        semver::Version::parse(version).map_err(fail)?;
        if image_sha256.len() != 64 || !image_sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(fail("invalid package fingerprint"));
        }
        private_directory(root)?;
        let root = root.canonicalize().map_err(fail)?;
        private_directory(&root.join("versions"))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(root.join("install.lock"))
            .map_err(fail)?;
        let metadata = lock.metadata().map_err(fail)?;
        if !metadata.is_file() || metadata.mode() & 0o077 != 0 {
            return Err(fail("invalid runtime installation lock"));
        }
        lock.try_lock_exclusive()
            .map_err(|_| fail("another runtime installation is in progress"))?;
        let destination = root.join("versions").join(image_sha256);
        let mut prepared = Self {
            root,
            stage: None,
            destination,
            identity: Identity {
                format: 1,
                version: version.into(),
                image_sha256: image_sha256.into(),
            },
            _lock: lock,
        };
        if prepared.destination.try_exists().map_err(fail)? {
            private_directory(&prepared.destination)?;
            let existing = identity(&prepared.destination)?;
            if existing.version != version || existing.image_sha256 != image_sha256 {
                return Err(fail(
                    "existing runtime identity conflicts with this package",
                ));
            }
            return Ok(prepared);
        }
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(fail)?;
        let name: String = random.iter().map(|v| format!("{v:02x}")).collect();
        let stage = prepared.root.join(format!(".stage-{name}"));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&stage)
            .map_err(fail)?;
        prepared.stage = Some(stage.clone());
        let source = source.canonicalize().map_err(fail)?;
        copy_tree(&source, &source, &stage, 0, &mut (0, 0))?;
        for required in ["AppRun", "usr/bin/Patina", "usr/bin/patinad"] {
            let canonical = stage.join(required).canonicalize().map_err(fail)?;
            if !canonical.starts_with(&stage)
                || !canonical.is_file()
                || fs::metadata(canonical).map_err(fail)?.mode() & 0o100 == 0
            {
                return Err(fail(
                    "bundled launcher or binaries are missing or not executable",
                ));
            }
        }
        let mut marker = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(stage.join(MARKER))
            .map_err(fail)?;
        serde_json::to_writer(&mut marker, &prepared.identity).map_err(fail)?;
        marker.flush().map_err(fail)?;
        marker.sync_all().map_err(fail)?;
        File::open(&stage).map_err(fail)?.sync_all().map_err(fail)?;
        Ok(prepared)
    }

    pub(crate) fn launcher(&self) -> PathBuf {
        self.stage
            .as_ref()
            .unwrap_or(&self.destination)
            .join("AppRun")
    }

    pub(crate) async fn verify(&self) -> Result<(), String> {
        verify_launcher(
            &self.launcher(),
            &self.identity.version,
            std::time::Duration::from_secs(10),
        )
        .await
    }

    /// Called only after the staged launcher reports the expected daemon version.
    pub(crate) fn publish(mut self) -> Result<PathBuf, String> {
        let current = self.root.join("current");
        match fs::symlink_metadata(&current) {
            Ok(meta) => {
                if !meta.file_type().is_symlink() {
                    return Err(fail("current runtime is not a managed link"));
                }
                let previous = current.canonicalize().map_err(fail)?;
                if previous.parent() != Some(self.root.join("versions").as_path()) {
                    return Err(fail(
                        "current runtime link escapes the managed versions directory",
                    ));
                }
                let old = identity(&previous)?;
                let old_version = semver::Version::parse(&old.version).map_err(fail)?;
                let new_version = semver::Version::parse(&self.identity.version).map_err(fail)?;
                if old_version > new_version {
                    return Ok(current.join("AppRun"));
                }
                if old_version == new_version && old.image_sha256 != self.identity.image_sha256 {
                    return Err(fail("different package content has the same version; use a new candidate version"));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(fail(error)),
        }
        if let Some(stage) = self.stage.as_ref() {
            fs::rename(stage, &self.destination).map_err(fail)?;
            self.stage = None;
            File::open(self.root.join("versions"))
                .map_err(fail)?
                .sync_all()
                .map_err(fail)?;
        }
        let temporary = self.root.join("current.next");
        // An interrupted pointer switch can be retried, but never remove an
        // unrelated regular file or a link to somewhere outside this store.
        if let Ok(meta) = fs::symlink_metadata(&temporary) {
            let target = fs::read_link(&temporary).map_err(fail)?;
            if !meta.file_type().is_symlink() || !valid_relative_version(&target) {
                return Err(fail("unrecognized pending runtime pointer"));
            }
            fs::remove_file(&temporary).map_err(fail)?;
        }
        symlink(
            Path::new("versions").join(&self.identity.image_sha256),
            &temporary,
        )
        .map_err(fail)?;
        fs::rename(&temporary, &current).map_err(fail)?;
        File::open(&self.root)
            .map_err(fail)?
            .sync_all()
            .map_err(fail)?;
        Ok(current.join("AppRun"))
    }
}

fn preflight_command(launcher: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(launcher);
    // A Desktop launched from another AppDir carries mount-specific loader paths.
    // The staged launcher must establish its own paths, as a fresh service would.
    for key in [
        "APPIMAGE",
        "ARGV0",
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "GTK_PATH",
        "GTK_EXE_PREFIX",
        "GTK_DATA_PREFIX",
        "GTK_IM_MODULE_FILE",
        "GDK_PIXBUF_MODULE_FILE",
        "GIO_EXTRA_MODULES",
        "GSETTINGS_SCHEMA_DIR",
    ] {
        command.env_remove(key);
    }
    command
        .env(
            "APPDIR",
            launcher.parent().expect("runtime launcher has a parent"),
        )
        .args(["--patinad", "--version"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    command
}

async fn verify_launcher(
    launcher: &Path,
    version: &str,
    deadline: std::time::Duration,
) -> Result<(), String> {
    use tokio::io::AsyncReadExt;
    tokio::time::timeout(deadline, async {
        let mut child = preflight_command(launcher).spawn().map_err(fail)?;
        let mut output = Vec::new();
        child
            .stdout
            .take()
            .expect("piped preflight stdout")
            .take(129)
            .read_to_end(&mut output)
            .await
            .map_err(fail)?;
        if output != format!("patinad {version}\n").as_bytes() {
            let _ = child.kill().await;
            return Err(fail(
                "runtime preflight returned an invalid version; existing runtime was not changed",
            ));
        }
        if !child.wait().await.map_err(fail)?.success() {
            return Err(fail(
                "runtime preflight failed; existing runtime was not changed",
            ));
        }
        Ok(())
    })
    .await
    .map_err(|_| fail("runtime preflight timed out; existing runtime was not changed"))?
}

impl Drop for PreparedRuntime {
    fn drop(&mut self) {
        if let Some(stage) = &self.stage {
            let _ = fs::remove_dir_all(stage);
        }
        // A concurrent spawn can briefly inherit this open file description.
        // Release ownership explicitly rather than waiting for every copy to close.
        let _ = FileExt::unlock(&self._lock);
    }
}

fn valid_relative_version(path: &Path) -> bool {
    let parts: Vec<_> = path.components().collect();
    parts.len() == 2
        && parts[0].as_os_str() == "versions"
        && parts[1]
            .as_os_str()
            .to_str()
            .is_some_and(|name| name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn copy_tree(
    root: &Path,
    source: &Path,
    target: &Path,
    depth: usize,
    budget: &mut (usize, u64),
) -> Result<(), String> {
    if depth > 64 {
        return Err(fail("package directory depth exceeds budget"));
    }
    for entry in fs::read_dir(source).map_err(fail)? {
        let entry = entry.map_err(fail)?;
        budget.0 += 1;
        if budget.0 > MAX_FILES {
            return Err(fail("package file count exceeds budget"));
        }
        let source_path = entry.path();
        let destination = target.join(entry.file_name());
        let meta = fs::symlink_metadata(&source_path).map_err(fail)?;
        if meta.file_type().is_symlink() {
            let resolved = source_path.canonicalize().map_err(fail)?;
            let relative = resolved
                .strip_prefix(root)
                .map_err(|_| fail("package link escapes AppDir"))?;
            let mut link = PathBuf::new();
            for _ in 0..depth {
                link.push("..");
            }
            link.push(relative);
            symlink(link, &destination).map_err(fail)?;
        } else if meta.is_dir() {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&destination)
                .map_err(fail)?;
            copy_tree(root, &source_path, &destination, depth + 1, budget)?;
            File::open(&destination)
                .map_err(fail)?
                .sync_all()
                .map_err(fail)?;
        } else if meta.is_file() {
            budget.1 = budget
                .1
                .checked_add(meta.len())
                .ok_or_else(|| fail("package size overflow"))?;
            if budget.1 > MAX_BYTES {
                return Err(fail("package size exceeds budget"));
            }
            let mut input = File::open(&source_path)
                .map_err(fail)?
                .take(meta.len().saturating_add(1));
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(if meta.mode() & 0o111 != 0 {
                    0o700
                } else {
                    0o600
                })
                .open(destination)
                .map_err(fail)?;
            if std::io::copy(&mut input, &mut output).map_err(fail)? != meta.len() {
                return Err(fail("package changed while copying"));
            }
            output.sync_all().map_err(fail)?;
        } else {
            return Err(fail("package contains a nonregular file"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    #[ignore = "requires an explicitly supplied built AppDir and AppImage; private runtime only"]
    async fn built_appdir_runs_from_durable_store() {
        let source = PathBuf::from(std::env::var_os("PATINA_APPIMAGE_TEST_SOURCE").unwrap());
        let image = PathBuf::from(std::env::var_os("PATINA_APPIMAGE_TEST_IMAGE").unwrap());
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let root = std::env::temp_dir().join(format!("patina-durable-appimage-{suffix}"));
        let hash = super::super::appimage_update::fingerprint(&image).unwrap();
        let prepared =
            PreparedRuntime::prepare(&source, &root, env!("CARGO_PKG_VERSION"), &hash).unwrap();
        prepared.verify().await.unwrap();
        let launcher = prepared.publish().unwrap();
        verify_launcher(
            &launcher,
            env!("CARGO_PKG_VERSION"),
            std::time::Duration::from_secs(10),
        )
        .await
        .unwrap();
        assert!(launcher
            .canonicalize()
            .unwrap()
            .starts_with(root.join("versions")));
        assert!(!root.join("data/Patina").exists());
        // Retain only this isolated store for the opt-in transient-systemd tests.
        println!("PERSISTED_APPDIR={}", launcher.parent().unwrap().display());
    }

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let mut random = [0u8; 8];
            getrandom::fill(&mut random).unwrap();
            let name: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
            let root = std::env::temp_dir().join(format!("patina-appdir-test-{name}"));
            fs::create_dir(&root).unwrap();
            fs::create_dir_all(root.join("source/usr/bin")).unwrap();
            for file in ["AppRun", "usr/bin/Patina", "usr/bin/patinad"] {
                let path = root.join("source").join(file);
                fs::write(&path, b"fixture executable").unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
            }
            Self(root)
        }
        fn prepare(&self, version: &str, hash: char) -> Result<PreparedRuntime, String> {
            PreparedRuntime::prepare(
                &self.0.join("source"),
                &self.0.join("store"),
                version,
                &hash.to_string().repeat(64),
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn preflight_rejects_bad_launchers_without_changing_current() {
        let fixture = Fixture::new();
        fs::write(
            fixture.0.join("source/AppRun"),
            "#!/bin/sh\nprintf 'patinad 1.9.0-beta.18\\n'\n",
        )
        .unwrap();
        let old = fixture
            .prepare("1.9.0-beta.18", 'a')
            .unwrap()
            .publish()
            .unwrap();
        let original = old.canonicalize().unwrap();
        for (index, script) in [
            "#!/bin/sh\nprintf 'patinad 0.0.0\\n'\n",
            "#!/bin/sh\nprintf 'patinad 1.9.0-beta.19\\n'\nexit 1\n",
            "#!/bin/sh\nwhile :; do printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'; done\n",
            "#!/bin/sh\nexec sleep 5\n",
        ]
        .iter()
        .enumerate()
        {
            fs::write(fixture.0.join("source/AppRun"), script).unwrap();
            let prepared = fixture
                .prepare("1.9.0-beta.19", char::from(b'b' + index as u8))
                .unwrap();
            let staged = prepared.launcher();
            assert!(verify_launcher(
                &staged,
                "1.9.0-beta.19",
                std::time::Duration::from_millis(200)
            )
            .await
            .is_err());
            drop(prepared);
            assert!(!staged.exists());
            assert_eq!(old.canonicalize().unwrap(), original);
            verify_launcher(&old, "1.9.0-beta.18", std::time::Duration::from_secs(2))
                .await
                .unwrap();
        }
        fs::write(fixture.0.join("source/AppRun"), "#!/bin/sh\n[ \"$APPDIR\" = \"$(dirname \"$0\")\" ] || exit 2\n[ -z \"${LD_LIBRARY_PATH+x}\" ] || exit 3\nprintf 'patinad 1.9.0-beta.19\\n'\n").unwrap();
        let prepared = fixture.prepare("1.9.0-beta.19", 'f').unwrap();
        prepared.verify().await.unwrap();
        prepared.publish().unwrap();
        assert_ne!(old.canonicalize().unwrap(), original);
    }

    #[test]
    fn preflight_replaces_mount_environment_without_mutating_parent() {
        let command = preflight_command(Path::new("/private/stage/AppRun"));
        let env: std::collections::HashMap<_, _> = command.as_std().get_envs().collect();
        assert_eq!(
            env[std::ffi::OsStr::new("APPDIR")],
            Some(std::ffi::OsStr::new("/private/stage"))
        );
        for key in [
            "APPIMAGE",
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "GTK_PATH",
            "GSETTINGS_SCHEMA_DIR",
        ] {
            assert_eq!(env[std::ffi::OsStr::new(key)], None);
        }
    }

    #[test]
    fn completed_install_releases_lock_even_with_inherited_descriptor() {
        let fixture = Fixture::new();
        let prepared = fixture.prepare("1.9.0-beta.18", 'a').unwrap();
        let inherited = prepared._lock.try_clone().unwrap();
        assert!(fixture.prepare("1.9.0-beta.19", 'b').is_err());
        prepared.publish().unwrap();
        let next = fixture.prepare("1.9.0-beta.19", 'b').unwrap();
        drop(inherited);
        assert!(fixture.prepare("1.9.0-beta.20", 'c').is_err());
        drop(next);
        fixture.prepare("1.9.0-beta.20", 'c').unwrap();
    }

    #[test]
    fn durable_versions_survive_source_removal_and_failed_upgrade() {
        let fixture = Fixture::new();
        symlink("patinad", fixture.0.join("source/usr/bin/helper")).unwrap();
        let first = fixture.prepare("1.9.0-beta.19", 'a').unwrap();
        let staged = first.launcher();
        assert!(staged.is_file());
        assert!(fixture
            .prepare("1.9.0-beta.20", 'b')
            .unwrap_err()
            .contains("in progress"));
        let launcher = first.publish().unwrap();
        let old = launcher.canonicalize().unwrap();
        assert!(fixture.0.join("store/current/usr/bin/helper").is_file());
        let cancelled = fixture.prepare("1.9.0-beta.20", 'b').unwrap();
        let cancelled_stage = cancelled.launcher();
        drop(cancelled);
        assert!(!cancelled_stage.exists());
        assert_eq!(launcher.canonicalize().unwrap(), old);
        assert!(fixture
            .prepare("1.9.0-beta.19", 'b')
            .unwrap()
            .publish()
            .is_err());
        fixture
            .prepare("1.9.0-beta.18", 'c')
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(launcher.canonicalize().unwrap(), old);
        symlink(
            Path::new("versions").join("d".repeat(64)),
            fixture.0.join("store/current.next"),
        )
        .unwrap();
        fixture
            .prepare("1.9.0-beta.20", 'b')
            .unwrap()
            .publish()
            .unwrap();
        assert_ne!(launcher.canonicalize().unwrap(), old);
        assert!(old.is_file());
        fs::remove_dir_all(fixture.0.join("source")).unwrap();
        assert!(launcher.is_file());
        assert!(fixture.0.join("store/current/usr/bin/helper").is_file());
    }

    #[test]
    fn unowned_paths_and_escaping_links_are_preserved_not_followed() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("unrelated"), b"keep").unwrap();
        symlink(fixture.0.join("unrelated"), fixture.0.join("source/escape")).unwrap();
        assert!(fixture
            .prepare("1.9.0", 'a')
            .unwrap_err()
            .contains("escapes"));
        assert_eq!(fs::read(fixture.0.join("unrelated")).unwrap(), b"keep");
        fs::remove_file(fixture.0.join("source/escape")).unwrap();
        fs::write(fixture.0.join("store/current"), b"not our pointer").unwrap();
        assert!(fixture.prepare("1.9.0", 'a').unwrap().publish().is_err());
        assert_eq!(
            fs::read(fixture.0.join("store/current")).unwrap(),
            b"not our pointer"
        );
        fs::remove_file(fixture.0.join("store/current")).unwrap();
        fs::write(fixture.0.join("store/current.next"), b"keep pending file").unwrap();
        assert!(fixture.prepare("1.9.0", 'a').unwrap().publish().is_err());
        assert_eq!(
            fs::read(fixture.0.join("store/current.next")).unwrap(),
            b"keep pending file"
        );
    }
}
