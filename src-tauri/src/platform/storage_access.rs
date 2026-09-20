//! Cross-process barrier for Desktop database handles and WebView storage.
//! A normal Desktop holds a shared lease for its process lifetime. Startup
//! maintenance must hold the exclusive lease before opening either resource.

use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::Duration;

#[derive(Debug)]
pub struct DesktopStorageAccess {
    file: File,
    exclusive: bool,
}

impl DesktopStorageAccess {
    pub async fn acquire(
        control_root: &Path,
        exclusive: bool,
        timeout: Duration,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(control_root).map_err(|error| {
            format!("failed to create Desktop storage control directory: {error}")
        })?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let file = options
            .open(control_root.join("desktop-storage.lock"))
            .map_err(|error| format!("failed to open Desktop storage access lock: {error}"))?;
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let result = if exclusive {
                FileExt::try_lock_exclusive(&file)
            } else {
                FileExt::try_lock_shared(&file)
            };
            match result {
                Ok(()) => return Ok(Self { file, exclusive }),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(format!("failed to lock Desktop storage: {error}")),
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("Desktop storage is still in use; close other Desktop instances and retry maintenance".to_string());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub fn is_exclusive(&self) -> bool {
        self.exclusive
    }

    pub fn share(&mut self) -> Result<(), String> {
        if self.exclusive {
            // flock may release the exclusive lock before acquiring the shared
            // lock. Another maintenance process can win that gap and then wait
            // for this Desktop to exit, so never block the startup thread here.
            FileExt::try_lock_shared(&self.file).map_err(|error| {
                if error.kind() == std::io::ErrorKind::WouldBlock {
                    "another Desktop acquired storage maintenance access; close other instances and retry startup".to_string()
                } else {
                    format!("failed to retain Desktop storage access: {error}")
                }
            })?;
            self.exclusive = false;
        }
        Ok(())
    }
}

impl Drop for DesktopStorageAccess {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

/// Compatibility barrier for clients released before `desktop-storage.lock`
/// and WebKit children that can outlive their Desktop. Call only while holding
/// exclusive Desktop access, and after stopping the runtime for data changes.
/// Other applications are checked on a best-effort basis; this is not an
/// exclusion protocol for arbitrary processes that do not cooperate with Patina.
pub async fn wait_for_legacy_storage_release(
    paths: &crate::platform::storage_paths::StoragePaths,
    data_changed: bool,
    webview_changed_or_cache: bool,
    timeout: Duration,
) -> Result<(), String> {
    if !data_changed && !webview_changed_or_cache {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        legacy::wait_for_release(
            Path::new("/proc"),
            paths,
            data_changed,
            webview_changed_or_cache,
            timeout,
        )
        .await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (paths, timeout);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod legacy {
    use crate::platform::storage_paths::StoragePaths;
    use std::ffi::OsString;
    use std::fs;
    use std::io::{BufRead, BufReader};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    pub(super) async fn wait_for_release(
        processes_root: &Path,
        paths: &StoragePaths,
        data_changed: bool,
        webview_changed_or_cache: bool,
        timeout: Duration,
    ) -> Result<(), String> {
        let protected = protected_paths(paths, data_changed, webview_changed_or_cache)?;
        let executable = std::env::current_exe()
            .map_err(|_| "could not identify the current Desktop executable".to_string())?;
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let Some(pid) = storage_user(
                processes_root,
                &protected,
                &executable,
                webview_changed_or_cache,
            )?
            else {
                return Ok(());
            };
            if tokio::time::Instant::now() >= deadline {
                return Err(format!(
                    "storage is still in use by process {pid}; close other Patina instances and wait for their WebKit processes to exit before retrying maintenance"
                ));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn protected_paths(
        paths: &StoragePaths,
        data: bool,
        webview: bool,
    ) -> Result<Vec<PathBuf>, String> {
        let mut protected = Vec::new();
        if data {
            let root = canonical_root(&paths.data_root)?;
            for name in ["patina.db", "patina.db-wal", "patina.db-shm", "backups"] {
                protected.push(root.join(name));
            }
        }
        if webview {
            let root = canonical_root(&paths.webview_root)?;
            // Same persistent entry allowlist as webview_cache::copy_persistent_profile,
            // plus SQLite sidecars and the cache directory cleared at restart.
            for name in [
                "localstorage",
                "storage",
                "CacheStorage",
                "mediakeys",
                "hsts-storage.sqlite",
                "hsts-storage.sqlite-wal",
                "hsts-storage.sqlite-shm",
                "cookies.sqlite",
                "cookies.sqlite-wal",
                "cookies.sqlite-shm",
                "databases",
                "IndexedDB",
                "ServiceWorker",
                "WebKitCache",
            ] {
                protected.push(root.join(name));
            }
        }
        Ok(protected)
    }

    fn canonical_root(path: &Path) -> Result<PathBuf, String> {
        match path.canonicalize() {
            Ok(path) => Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path.to_path_buf()),
            Err(_) => Err("could not resolve storage paths for the maintenance barrier".into()),
        }
    }

    fn storage_user(
        processes_root: &Path,
        protected: &[PathBuf],
        executable: &Path,
        webview: bool,
    ) -> Result<Option<u32>, String> {
        let processes = fs::read_dir(processes_root)
            .map_err(|_| "could not inspect processes before storage maintenance".to_string())?;
        let uid = unsafe { libc::geteuid() };
        for entry in processes {
            let entry = entry.map_err(|_| {
                "could not enumerate processes before storage maintenance".to_string()
            })?;
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|value| value.parse::<u32>().ok())
            else {
                continue;
            };
            if pid == std::process::id() {
                continue;
            }
            let process = entry.path();
            let Some((owner, name)) = process_identity(&process) else {
                continue;
            };
            if owner != uid {
                continue;
            }
            if name.eq_ignore_ascii_case(b"Patina") {
                return Ok(Some(pid));
            }
            // WebKit's Linux comm is truncated to 15 bytes (e.g. WebKitNetworkPr).
            // Unrelated non-dumpable applications such as keyrings must not
            // permanently block maintenance just because /proc denies access.
            let mut required = webview && name.starts_with(b"WebKit");
            if let Some(path) = inspect(fs::read_link(process.join("exe")), pid, required)? {
                let path = without_deleted_suffix(&path);
                if known_desktop(&path, executable) {
                    // An idle legacy client can reopen its pool later. Merely
                    // finding no open DB handle is not enough for that process.
                    return Ok(Some(pid));
                }
                required |= webview
                    && path
                        .file_name()
                        .is_some_and(|name| name.as_bytes().starts_with(b"WebKit"));
            }
            if let Some(descriptors) = inspect(fs::read_dir(process.join("fd")), pid, required)? {
                for descriptor in descriptors {
                    let Some(descriptor) = inspect(descriptor, pid, required)? else {
                        continue;
                    };
                    if let Some(path) = inspect(fs::read_link(descriptor.path()), pid, required)? {
                        if protects(&path, protected) {
                            return Ok(Some(pid));
                        }
                    }
                }
            }
            // A WebKit process can close a descriptor while retaining its mmap.
            // Read mapping metadata only; never read the mapped storage contents.
            if let Some(maps) = inspect(fs::File::open(process.join("maps")), pid, required)? {
                for line in BufReader::new(maps).split(b'\n') {
                    let Some(line) = inspect(line, pid, required)? else {
                        continue;
                    };
                    if mapped_path(&line).is_some_and(|path| {
                        protects(&path, protected)
                            || protects(&unescape_mapped_newline(&path), protected)
                    }) {
                        return Ok(Some(pid));
                    }
                }
            }
        }
        Ok(None)
    }

    fn process_identity(process: &Path) -> Option<(u32, Vec<u8>)> {
        // /proc directory ownership can become root for non-dumpable processes;
        // the status UID remains the actual effective UID in that case.
        let status = fs::File::open(process.join("status")).ok()?;
        let mut name = Vec::new();
        for line in BufReader::new(status).split(b'\n') {
            let line = line.ok()?;
            if let Some(value) = line.strip_prefix(b"Name:\t") {
                name = value.to_vec();
            }
            if let Some(state) = line.strip_prefix(b"State:\t") {
                if matches!(state.first(), Some(b'Z' | b'X')) {
                    return None; // Exited processes cannot reopen their storage.
                }
            }
            if let Some(uids) = line.strip_prefix(b"Uid:") {
                let uid = std::str::from_utf8(uids)
                    .ok()?
                    .split_whitespace()
                    .nth(1)?
                    .parse()
                    .ok()?;
                return Some((uid, name));
            }
        }
        None
    }

    fn inspect<T>(
        result: std::io::Result<T>,
        pid: u32,
        required: bool,
    ) -> Result<Option<T>, String> {
        match result {
            Ok(value) => Ok(Some(value)),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(libc::ESRCH) =>
            {
                Ok(None)
            }
            Err(_) if !required => Ok(None),
            Err(_) => Err(format!(
                "could not verify storage use by process {pid}; maintenance was not started"
            )),
        }
    }

    fn known_desktop(path: &Path, executable: &Path) -> bool {
        path == executable
            || path
                .file_name()
                .is_some_and(|name| name.as_bytes().eq_ignore_ascii_case(b"Patina"))
    }

    fn without_deleted_suffix(path: &Path) -> PathBuf {
        let bytes = path.as_os_str().as_bytes();
        PathBuf::from(OsString::from_vec(
            bytes.strip_suffix(b" (deleted)").unwrap_or(bytes).to_vec(),
        ))
    }

    fn protects(path: &Path, protected: &[PathBuf]) -> bool {
        let path = without_deleted_suffix(path);
        protected.iter().any(|root| path.starts_with(root))
    }

    fn mapped_path(line: &[u8]) -> Option<PathBuf> {
        let mut position = 0;
        for _ in 0..5 {
            while line.get(position).is_some_and(u8::is_ascii_whitespace) {
                position += 1;
            }
            while line
                .get(position)
                .is_some_and(|byte| !byte.is_ascii_whitespace())
            {
                position += 1;
            }
        }
        while line.get(position).is_some_and(u8::is_ascii_whitespace) {
            position += 1;
        }
        if line.get(position) != Some(&b'/') {
            return None;
        }
        Some(PathBuf::from(OsString::from_vec(line[position..].to_vec())))
    }

    fn unescape_mapped_newline(path: &Path) -> PathBuf {
        // Linux escapes newlines as \\012 in maps; other path characters,
        // including spaces and literal backslashes, are preserved.
        let bytes = path.as_os_str().as_bytes();
        let mut path = Vec::new();
        let mut position = 0;
        while position < bytes.len() {
            if bytes[position..].starts_with(b"\\012") {
                path.push(b'\n');
                position += 4;
            } else {
                path.push(bytes[position]);
                position += 1;
            }
        }
        PathBuf::from(OsString::from_vec(path))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn unreadable_metadata_only_blocks_relevant_candidates() {
            let denied = || Err::<(), _>(std::io::Error::from_raw_os_error(libc::EACCES));
            assert!(inspect(denied(), 42, false).unwrap().is_none());
            assert!(inspect(denied(), 42, true)
                .unwrap_err()
                .contains("process 42"));

            let root = super::super::tests::root("proc-candidates");
            let process = root.join("42424242");
            fs::create_dir_all(&process).unwrap();
            // A symlink loop reliably makes inspection fail even in root-run CI.
            std::os::unix::fs::symlink("fd", process.join("fd")).unwrap();
            let status = |name: &str| {
                format!(
                    "Name:\t{name}\nUid:\t{}\t{}\t{}\t{}\n",
                    unsafe { libc::geteuid() },
                    unsafe { libc::geteuid() },
                    unsafe { libc::geteuid() },
                    unsafe { libc::geteuid() }
                )
            };
            fs::write(process.join("status"), status("gnome-keyring-d")).unwrap();
            assert!(storage_user(&root, &[], Path::new("/unrelated"), true)
                .unwrap()
                .is_none());
            fs::write(process.join("status"), status("WebKitNetworkPr")).unwrap();
            assert!(storage_user(&root, &[], Path::new("/unrelated"), true).is_err());
            assert!(storage_user(&root, &[], Path::new("/unrelated"), false)
                .unwrap()
                .is_none());
            fs::remove_dir_all(root).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "patina-storage-access-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn legacy_storage_process_fixture() {
        use std::os::fd::AsRawFd;
        let Some(root) = std::env::var_os("PATINA_TEST_LEGACY_RESOURCE_ROOT") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let path = std::env::var_os("PATINA_TEST_LEGACY_RESOURCE_PATH").unwrap();
        let mode = std::env::var("PATINA_TEST_LEGACY_RESOURCE_MODE").unwrap();
        let mut file = (mode != "idle").then(|| File::open(path).unwrap());
        let mapping = if mode == "mapping" {
            let mapping = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    4096,
                    libc::PROT_READ,
                    libc::MAP_SHARED,
                    file.as_ref().unwrap().as_raw_fd(),
                    0,
                )
            };
            assert_ne!(mapping, libc::MAP_FAILED);
            file.take(); // Only /proc/maps can find this resource now.
            Some(mapping)
        } else {
            None
        };
        std::fs::write(root.join("ready"), b"ready").unwrap();
        while !root.join("release").exists() {
            std::thread::sleep(Duration::from_millis(10));
        }
        if let Some(mapping) = mapping {
            unsafe {
                libc::munmap(mapping, 4096);
            }
        }
        drop(file);
    }

    #[cfg(target_os = "linux")]
    struct LegacyFixture {
        root: std::path::PathBuf,
        proc_view: std::path::PathBuf,
        resource: std::path::PathBuf,
        paths: crate::platform::storage_paths::StoragePaths,
        child: std::process::Child,
    }

    #[cfg(target_os = "linux")]
    impl LegacyFixture {
        async fn start(mode: &str, relative_resource: &str, executable_name: &str) -> Self {
            let root = root("legacy-process");
            let storage = root.join("shared storage");
            let resource = storage.join(relative_resource);
            std::fs::create_dir_all(resource.parent().unwrap()).unwrap();
            std::fs::write(&resource, vec![0_u8; 4096]).unwrap();
            let executable = root.join(executable_name);
            // Distinct pathname lets FD-only fixtures avoid the known Desktop
            // executable rule while still executing the real Rust test helper.
            std::fs::hard_link(std::env::current_exe().unwrap(), &executable)
                .or_else(|_| {
                    std::fs::copy(std::env::current_exe().unwrap(), &executable).map(|_| ())
                })
                .unwrap();
            let child = std::process::Command::new(&executable)
                .args([
                    "--exact",
                    "platform::storage_access::tests::legacy_storage_process_fixture",
                    "--nocapture",
                ])
                .env("PATINA_TEST_LEGACY_RESOURCE_ROOT", &root)
                .env("PATINA_TEST_LEGACY_RESOURCE_PATH", &resource)
                .env("PATINA_TEST_LEGACY_RESOURCE_MODE", mode)
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let proc_view = root.join("proc");
            std::fs::create_dir(&proc_view).unwrap();
            std::os::unix::fs::symlink(
                format!("/proc/{}", child.id()),
                proc_view.join(child.id().to_string()),
            )
            .unwrap();
            let paths = crate::platform::storage_paths::StoragePaths::from_roots(
                root.join("control"),
                storage.clone(),
                storage.clone(),
                storage,
                false,
                false,
            );
            let mut fixture = Self {
                root,
                proc_view,
                resource,
                paths,
                child,
            };
            tokio::time::timeout(Duration::from_secs(5), async {
                while !fixture.root.join("ready").exists() {
                    assert!(
                        fixture.child.try_wait().unwrap().is_none(),
                        "legacy fixture exited early"
                    );
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("legacy fixture should become ready");
            fixture
        }

        async fn check(&self, data: bool, webview: bool, timeout: Duration) -> Result<(), String> {
            // The view contains only our owned child, but its status, descriptors
            // and mappings are real /proc data and use the production scanner.
            legacy::wait_for_release(&self.proc_view, &self.paths, data, webview, timeout).await
        }

        fn release(&self) {
            std::fs::write(self.root.join("release"), b"release").unwrap();
        }
    }

    #[cfg(target_os = "linux")]
    impl Drop for LegacyFixture {
        fn drop(&mut self) {
            let _ = self.child.kill(); // Only this test-owned synthetic child.
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn legacy_database_handle_blocks_data_but_not_webview_maintenance() {
        let fixture = LegacyFixture::start("fd", "patina.db-wal", "storage-reader").await;
        fixture.check(false, true, Duration::ZERO).await.unwrap();
        let error = fixture
            .check(true, false, Duration::ZERO)
            .await
            .unwrap_err();
        assert!(error.contains(&format!("process {}", fixture.child.id())));
        let release = async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            fixture.release();
        };
        let (result, ()) =
            tokio::join!(fixture.check(true, false, Duration::from_secs(2)), release);
        result.unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn legacy_closed_fd_mapping_and_deleted_cache_remain_protected() {
        let fixture = LegacyFixture::start(
            "mapping",
            "WebKitCache/Version 17/cache\nmap",
            "storage-reader",
        )
        .await;
        fixture.check(true, false, Duration::ZERO).await.unwrap();
        assert!(fixture.check(false, true, Duration::ZERO).await.is_err());
        std::fs::remove_file(&fixture.resource).unwrap();
        assert!(fixture.check(false, true, Duration::ZERO).await.is_err());
        fixture.release();
        fixture
            .check(false, true, Duration::from_secs(2))
            .await
            .unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn legacy_idle_desktop_must_exit_before_it_can_reopen_storage() {
        let fixture = LegacyFixture::start("idle", "patina.db", "Patina").await;
        assert!(fixture.check(true, false, Duration::ZERO).await.is_err());
        fixture.release();
        fixture
            .check(true, false, Duration::from_secs(2))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn maintenance_waits_for_every_desktop_and_blocks_new_readers() {
        let root = root("barrier");
        let first = DesktopStorageAccess::acquire(&root, false, Duration::ZERO)
            .await
            .unwrap();
        let second = DesktopStorageAccess::acquire(&root, false, Duration::ZERO)
            .await
            .unwrap();
        assert!(DesktopStorageAccess::acquire(&root, true, Duration::ZERO)
            .await
            .is_err());
        drop(first);
        assert!(DesktopStorageAccess::acquire(&root, true, Duration::ZERO)
            .await
            .is_err());
        drop(second);
        let mut maintenance = DesktopStorageAccess::acquire(&root, true, Duration::ZERO)
            .await
            .unwrap();
        assert!(DesktopStorageAccess::acquire(&root, false, Duration::ZERO)
            .await
            .is_err());
        maintenance.share().unwrap();
        let reader = DesktopStorageAccess::acquire(&root, false, Duration::ZERO)
            .await
            .unwrap();
        drop(reader);
        drop(maintenance);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn contended_storage_downgrade_returns_without_blocking() {
        const CHILD_ROOT: &str = "PATINA_TEST_STORAGE_DOWNGRADE_CHILD_ROOT";
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            // Model the conversion gap: this handle has lost its exclusive
            // lock while the parent has acquired maintenance ownership.
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(std::path::PathBuf::from(root).join("desktop-storage.lock"))
                .unwrap();
            let mut access = DesktopStorageAccess {
                file,
                exclusive: true,
            };
            let error = access.share().unwrap_err();
            assert!(error.contains("retry startup"));
            return;
        }
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let root = root("downgrade-contention");
        std::fs::create_dir_all(&root).unwrap();
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("desktop-storage.lock"))
            .unwrap();
        FileExt::lock_exclusive(&owner).unwrap();
        let mut child = Child(std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "platform::storage_access::tests::contended_storage_downgrade_returns_without_blocking", "--nocapture"])
            .env(CHILD_ROOT, &root)
            .stdout(std::process::Stdio::null()).spawn().unwrap());
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success(), "storage downgrade fixture failed");
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "contended downgrade must return instead of blocking on another maintenance owner"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(child);
        drop(owner);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn lock_symlinks_are_rejected_without_touching_the_target() {
        let root = root("symlink");
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("untouched");
        std::fs::write(&target, b"preserved").unwrap();
        std::os::unix::fs::symlink(&target, root.join("desktop-storage.lock")).unwrap();
        assert!(DesktopStorageAccess::acquire(&root, true, Duration::ZERO)
            .await
            .is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"preserved");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn exited_desktop_process_releases_the_storage_barrier() {
        const CHILD_ROOT: &str = "PATINA_TEST_STORAGE_ACCESS_CHILD_ROOT";
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let root = std::path::PathBuf::from(root);
            let _access = DesktopStorageAccess::acquire(&root, false, Duration::ZERO)
                .await
                .unwrap();
            std::fs::write(root.join("child-ready"), b"ready").unwrap();
            std::future::pending::<()>().await;
            return;
        }
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let root = root("process-exit");
        let mut child = Child(std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "platform::storage_access::tests::exited_desktop_process_releases_the_storage_barrier", "--nocapture"])
            .env(CHILD_ROOT, &root)
            .stdout(std::process::Stdio::null())
            .spawn().unwrap());
        tokio::time::timeout(Duration::from_secs(5), async {
            while !root.join("child-ready").exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "storage fixture exited early"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("child should acquire its Desktop storage lease");
        assert!(DesktopStorageAccess::acquire(&root, true, Duration::ZERO)
            .await
            .is_err());
        // Only this test-owned child is terminated; a crashed Desktop must not
        // leave a stale PID marker that prevents later offline maintenance.
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let maintenance = DesktopStorageAccess::acquire(&root, true, Duration::ZERO)
            .await
            .unwrap();
        drop(maintenance);
        std::fs::remove_dir_all(root).unwrap();
    }
}
