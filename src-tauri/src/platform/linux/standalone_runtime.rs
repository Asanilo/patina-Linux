//! Versioned standalone files and selected target. No service control, execution or profile IO.
use fs2::FileExt;
use patina_protocol::build_info::{DaemonBuildInfo, BUILD_INFO_FORMAT_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

mod selection;
pub(crate) use selection::{inspect, select, ExpectedCurrent};

const ROOT_MARKER: &str = ".patina-standalone-root";
const ROOT_IDENTITY: &[u8] = b"Patina standalone runtime root v1\n";
const MANIFEST_LIMIT: u64 = 64 * 1024;
const PAYLOADS: [(&str, u32, u64); 4] = [
    ("bin/patinad", 0o755, 512 * 1024 * 1024),
    ("systemd/patinad.service.in", 0o644, 16 * 1024),
    ("README.txt", 0o644, 64 * 1024),
    ("LICENSE", 0o644, 64 * 1024),
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileIdentity {
    sha256: String,
    size: u64,
    mode: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format_version: u32,
    distribution: String,
    build: DaemonBuildInfo,
    files: BTreeMap<String, FileIdentity>,
}

#[derive(Debug, Serialize)]
pub(crate) struct StagedRuntime {
    pub manifest_sha256: String,
    pub binary_sha256: String,
    pub directory: PathBuf,
    pub build: DaemonBuildInfo,
}

fn error(value: impl std::fmt::Display) -> String {
    format!("standalone runtime: {value}")
}
fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn uid() -> u32 {
    // SAFETY: geteuid takes no pointers and has no side effects.
    unsafe { libc::geteuid() }
}

fn directory(path: &Path, private: bool) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(error)?;
    if !metadata.is_dir() || (private && (metadata.uid() != uid() || metadata.mode() & 0o077 != 0))
    {
        return Err(error("expected a directory without links; installation directories must be private and user-owned"));
    }
    Ok(())
}

fn private_directory(path: &Path) -> Result<(), String> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(failure) if failure.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(failure) => return Err(error(failure)),
    }
    directory(path, true)
}

fn open_regular(path: &Path, limit: u64) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(error)?;
    let metadata = file.metadata().map_err(error)?;
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > limit {
        return Err(error("invalid, linked or oversized payload file"));
    }
    Ok(file)
}

fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    open_regular(path, limit)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() as u64 > limit {
        return Err(error("oversized metadata"));
    }
    Ok(bytes)
}

fn entries(path: &Path, expected: &[&str]) -> Result<(), String> {
    directory(path, false)?;
    let actual = fs::read_dir(path)
        .map_err(error)?
        .map(|entry| {
            entry
                .map_err(error)?
                .file_name()
                .into_string()
                .map_err(|_| error("non-UTF8 payload name"))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if actual != expected.iter().map(|name| name.to_string()).collect() {
        return Err(error("unexpected or missing payload entries"));
    }
    Ok(())
}

fn file_digest(path: &Path, identity: &FileIdentity) -> Result<String, String> {
    let mut file = open_regular(path, identity.size)?;
    let metadata = file.metadata().map_err(error)?;
    if metadata.len() != identity.size || metadata.mode() & 0o7777 != identity.mode {
        return Err(error("payload size or mode differs from manifest"));
    }
    let mut hash = Sha256::new();
    let copied = std::io::copy(
        &mut (&mut file).take(identity.size + 1),
        &mut HashWriter(&mut hash),
    )
    .map_err(error)?;
    if copied != identity.size {
        return Err(error("payload changed while reading"));
    }
    Ok(format!("{:x}", hash.finalize()))
}

struct HashWriter<'a>(&'a mut Sha256);
impl Write for HashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn verify(source: &Path, expected: &str, allow_debug: bool) -> Result<(Manifest, Vec<u8>), String> {
    if !is_digest(expected) {
        return Err(error("expected manifest SHA256 is invalid"));
    }
    entries(
        source,
        &["manifest.json", "bin", "systemd", "README.txt", "LICENSE"],
    )?;
    entries(&source.join("bin"), &["patinad"])?;
    entries(&source.join("systemd"), &["patinad.service.in"])?;
    let bytes = read_regular(&source.join("manifest.json"), MANIFEST_LIMIT)?;
    if digest(&bytes) != expected {
        return Err(error("manifest SHA256 mismatch"));
    }
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(error)?;
    let build = &manifest.build;
    if manifest.format_version != 1
        || manifest.distribution != "standalone"
        || build.format_version != BUILD_INFO_FORMAT_VERSION
        || build.desktop_feature
        || (build.debug_assertions && !allow_debug)
        || build.target != env!("PATINA_BUILD_TARGET")
        || build.protocol.min_supported_client == 0
        || !(build.protocol.min_supported_client..=build.protocol.max_supported_client)
            .contains(&build.protocol.current)
    {
        return Err(error(
            "unsupported runtime manifest, target or build projection",
        ));
    }
    semver::Version::parse(&build.package_version).map_err(error)?;
    if manifest.files.len() != PAYLOADS.len() {
        return Err(error("unexpected manifest file set"));
    }
    for (name, mode, maximum) in PAYLOADS {
        let identity = manifest
            .files
            .get(name)
            .ok_or_else(|| error("missing payload identity"))?;
        if identity.mode != mode
            || identity.size == 0
            || identity.size > maximum
            || !is_digest(&identity.sha256)
            || file_digest(&source.join(name), identity)? != identity.sha256
        {
            return Err(error(format!("invalid payload identity for {name}")));
        }
    }
    let mut magic = [0u8; 4];
    open_regular(&source.join("bin/patinad"), PAYLOADS[0].2)?
        .read_exact(&mut magic)
        .map_err(error)?;
    if magic != *b"\x7fELF" {
        return Err(error("daemon payload is not an ELF executable"));
    }
    Ok((manifest, bytes))
}

struct TemporaryDirectory(Option<PathBuf>);
impl TemporaryDirectory {
    fn create(parent: &Path, prefix: &str) -> Result<Self, String> {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(error)?;
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let path = parent.join(format!(".{prefix}-{suffix}"));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(error)?;
        Ok(Self(Some(path)))
    }
    fn path(&self) -> &Path {
        self.0
            .as_deref()
            .expect("temporary directory is unpublished")
    }
    fn publish(&mut self, destination: &Path) -> Result<(), String> {
        fs::rename(self.path(), destination).map_err(error)?;
        self.0 = None;
        Ok(())
    }
}
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn write_new(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(error)?;
    file.write_all(bytes).map_err(error)?;
    file.sync_all().map_err(error)
}

fn validate_root(root: &Path) -> Result<(), String> {
    directory(root, true)?;
    let metadata = fs::symlink_metadata(root.join(ROOT_MARKER)).map_err(error)?;
    if metadata.uid() != uid() || metadata.mode() & 0o7777 != 0o600 {
        return Err(error("invalid runtime root marker ownership or mode"));
    }
    if read_regular(&root.join(ROOT_MARKER), 1024)? != ROOT_IDENTITY {
        return Err(error("unrecognized runtime root"));
    }
    Ok(())
}

fn initialize_root(root: &Path) -> Result<(), String> {
    if fs::symlink_metadata(root).is_ok() {
        directory(root, true)?;
        if fs::read_dir(root).map_err(error)?.next().is_some() {
            return validate_root(root);
        }
    }
    let parent = root
        .parent()
        .ok_or_else(|| error("runtime root has no parent"))?;
    let mut temporary = TemporaryDirectory::create(parent, "patina-runtime-init")?;
    write_new(&temporary.path().join(ROOT_MARKER), ROOT_IDENTITY, 0o600)?;
    File::open(temporary.path())
        .and_then(|file| file.sync_all())
        .map_err(error)?;
    if let Err(failure) = temporary.publish(root) {
        // A concurrent initializer may have published the same root first.
        if validate_root(root).is_err() {
            return Err(failure);
        }
    }
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(error)?;
    validate_root(root)
}

fn validate_root_path(root: &Path) -> Result<(), String> {
    if !root.is_absolute()
        || root
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        || root
            .to_str()
            .is_none_or(|value| value.chars().any(char::is_control))
    {
        return Err(error(
            "runtime root must be an absolute UTF-8 path without traversal or control characters",
        ));
    }
    Ok(())
}

fn installation_lock(root: &Path, create: bool, exclusive: bool) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(exclusive)
        .create(create)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let lock = options.open(root.join("install.lock")).map_err(error)?;
    let metadata = lock.metadata().map_err(error)?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != uid()
        || metadata.mode() & 0o7777 != 0o600
    {
        return Err(error("invalid installation lock"));
    }
    let result = if exclusive {
        FileExt::try_lock_exclusive(&lock)
    } else {
        FileExt::try_lock_shared(&lock)
    };
    result.map_err(|_| error("another runtime installation is in progress"))?;
    Ok(lock)
}

fn installed_version(
    root: &Path,
    expected: &str,
    allow_debug: bool,
) -> Result<StagedRuntime, String> {
    if !is_digest(expected) {
        return Err(error("invalid installed manifest identity"));
    }
    let destination = root.join("versions").join(expected);
    for path in [
        root.join("versions"),
        destination.clone(),
        destination.join("bin"),
        destination.join("systemd"),
    ] {
        directory(&path, true)?;
    }
    let (manifest, _) = verify(&destination, expected, allow_debug)?;
    for name in [
        "manifest.json",
        "bin/patinad",
        "systemd/patinad.service.in",
        "README.txt",
        "LICENSE",
    ] {
        if fs::symlink_metadata(destination.join(name))
            .map_err(error)?
            .uid()
            != uid()
        {
            return Err(error("installed payload must be owned by the current user"));
        }
    }
    Ok(StagedRuntime {
        manifest_sha256: expected.to_owned(),
        binary_sha256: manifest.files["bin/patinad"].sha256.clone(),
        directory: destination,
        build: manifest.build,
    })
}

pub(crate) fn stage(
    source: &Path,
    root: &Path,
    expected: &str,
    allow_debug: bool,
) -> Result<StagedRuntime, String> {
    validate_root_path(root)?;
    let (manifest, bytes) = verify(source, expected, allow_debug)?;
    initialize_root(root)?;
    let root = root.canonicalize().map_err(error)?;
    let _lock = installation_lock(&root, true, true)?;
    let versions = root.join("versions");
    private_directory(&versions)?;
    let destination = versions.join(expected);
    match fs::symlink_metadata(&destination) {
        Ok(_) => {
            return installed_version(&root, expected, allow_debug);
        }
        Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => {
            let mut temporary = TemporaryDirectory::create(&root, "stage")?;
            private_directory(&temporary.path().join("bin"))?;
            private_directory(&temporary.path().join("systemd"))?;
            for (name, identity) in &manifest.files {
                let mut input =
                    open_regular(&source.join(name), identity.size)?.take(identity.size + 1);
                let target = temporary.path().join(name);
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&target)
                    .map_err(error)?;
                if std::io::copy(&mut input, &mut output).map_err(error)? != identity.size {
                    return Err(error("payload size changed during staging"));
                }
                output
                    .set_permissions(fs::Permissions::from_mode(identity.mode))
                    .map_err(error)?;
                output.sync_all().map_err(error)?;
            }
            write_new(&temporary.path().join("manifest.json"), &bytes, 0o644)?;
            verify(temporary.path(), expected, allow_debug)?;
            for path in [
                temporary.path().join("bin"),
                temporary.path().join("systemd"),
                temporary.path().to_path_buf(),
            ] {
                File::open(path)
                    .and_then(|file| file.sync_all())
                    .map_err(error)?;
            }
            temporary.publish(&destination)?;
            File::open(&versions)
                .and_then(|file| file.sync_all())
                .map_err(error)?;
            File::open(&root)
                .and_then(|file| file.sync_all())
                .map_err(error)?;
        }
        Err(failure) => return Err(error(failure)),
    }
    Ok(StagedRuntime {
        manifest_sha256: expected.to_owned(),
        binary_sha256: manifest.files["bin/patinad"].sha256.clone(),
        directory: destination,
        build: manifest.build,
    })
}

#[cfg(test)]
mod tests;
