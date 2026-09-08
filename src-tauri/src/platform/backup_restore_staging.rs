use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::domain::backup::MAX_BACKUP_ARCHIVE_BYTES;

const TICKET_BYTES: usize = 16;
const TICKET_HEX_LEN: usize = TICKET_BYTES * 2;
const COPY_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StagedBackupArchive {
    pub ticket: String,
    pub sha256: String,
    pub size_bytes: u64,
}

pub fn stage_file(root: &Path, source_path: &Path) -> Result<StagedBackupArchive, String> {
    validate_source_file(source_path)?;
    prepare_root(root)?;

    let mut source = File::open(source_path)
        .map_err(|error| format!("failed to open backup archive for staging: {error}"))?;
    let source_metadata = source
        .metadata()
        .map_err(|error| format!("failed to inspect opened backup archive: {error}"))?;
    if !source_metadata.is_file() {
        return Err("backup archive source must be a regular file".to_string());
    }
    if source_metadata.len() > MAX_BACKUP_ARCHIVE_BYTES {
        return Err(size_error());
    }

    for _ in 0..32 {
        let ticket = random_ticket()?;
        let path = staged_path_unchecked(root, &ticket);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut target = match options.open(&path) {
            Ok(target) => target,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("failed to create staged backup archive: {error}"));
            }
        };

        let result = copy_and_hash(&mut source, &mut target).and_then(|(sha256, size_bytes)| {
            target
                .sync_all()
                .map_err(|error| format!("failed to sync staged backup archive: {error}"))?;
            enforce_file_permissions(&path)?;
            sync_directory(root)?;
            Ok(StagedBackupArchive {
                ticket: ticket.clone(),
                sha256,
                size_bytes,
            })
        });
        if result.is_err() {
            let _ = fs::remove_file(&path);
        }
        return result;
    }

    Err("failed to allocate a backup restore staging ticket".to_string())
}

pub fn validate(
    root: &Path,
    ticket: &str,
    expected_sha256: &str,
    expected_size_bytes: u64,
) -> Result<PathBuf, String> {
    validate_existing_root(root)?;
    validate_fingerprint(expected_sha256)?;
    let path = staged_path(root, ticket)?;
    validate_staged_file(&path)?;
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("failed to inspect staged backup archive: {error}"))?;
    if metadata.len() != expected_size_bytes {
        return Err("staged backup archive size changed after preview".to_string());
    }

    let mut file = File::open(&path)
        .map_err(|error| format!("failed to open staged backup archive: {error}"))?;
    let (actual_sha256, actual_size) = hash_reader(&mut file)?;
    if actual_size != expected_size_bytes || actual_sha256 != expected_sha256 {
        return Err("staged backup archive does not match the preview fingerprint".to_string());
    }
    Ok(path)
}

pub fn discard(root: &Path, ticket: &str) -> Result<(), String> {
    validate_ticket(ticket)?;
    match fs::symlink_metadata(root) {
        Ok(_) => validate_existing_root(root)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "backup restore staging directory is unavailable: {error}"
            ));
        }
    }
    let path = staged_path_unchecked(root, ticket);
    match fs::remove_file(path) {
        Ok(()) => {
            sync_directory(root)?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to discard staged backup archive: {error}")),
    }
}

fn copy_and_hash(source: &mut File, target: &mut File) -> Result<(String, u64), String> {
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    loop {
        let count = source
            .read(&mut buffer)
            .map_err(|error| format!("failed to read backup archive for staging: {error}"))?;
        if count == 0 {
            break;
        }
        total = total.checked_add(count as u64).ok_or_else(size_error)?;
        if total > MAX_BACKUP_ARCHIVE_BYTES {
            return Err(size_error());
        }
        target
            .write_all(&buffer[..count])
            .map_err(|error| format!("failed to write staged backup archive: {error}"))?;
        hasher.update(&buffer[..count]);
    }
    Ok((format!("{:x}", hasher.finalize()), total))
}

fn hash_reader(file: &mut File) -> Result<(String, u64), String> {
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to read staged backup archive: {error}"))?;
        if count == 0 {
            break;
        }
        total = total.checked_add(count as u64).ok_or_else(size_error)?;
        if total > MAX_BACKUP_ARCHIVE_BYTES {
            return Err(size_error());
        }
        hasher.update(&buffer[..count]);
    }
    Ok((format!("{:x}", hasher.finalize()), total))
}

fn validate_source_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("backup archive is unavailable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("backup archive source must be a regular non-symlink file".to_string());
    }
    if metadata.len() > MAX_BACKUP_ARCHIVE_BYTES {
        return Err(size_error());
    }
    Ok(())
}

fn prepare_root(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root)
        .map_err(|error| format!("failed to create backup restore staging directory: {error}"))?;
    validate_existing_root(root)?;
    enforce_directory_permissions(root)
}

fn validate_existing_root(root: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("backup restore staging directory is unavailable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("backup restore staging path must be a real directory".to_string());
    }
    Ok(())
}

fn validate_staged_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("staged backup archive is unavailable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("staged backup archive must be a regular file".to_string());
    }
    if metadata.len() > MAX_BACKUP_ARCHIVE_BYTES {
        return Err(size_error());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.nlink() != 1 {
            return Err("staged backup archive must not have hard links".to_string());
        }
        if metadata.permissions().mode() & 0o777 != 0o600 {
            return Err("staged backup archive permissions must be 0600".to_string());
        }
    }
    Ok(())
}

fn staged_path(root: &Path, ticket: &str) -> Result<PathBuf, String> {
    validate_ticket(ticket)?;
    Ok(staged_path_unchecked(root, ticket))
}

fn staged_path_unchecked(root: &Path, ticket: &str) -> PathBuf {
    root.join(format!("{ticket}.zip"))
}

fn validate_ticket(ticket: &str) -> Result<(), String> {
    if ticket.len() != TICKET_HEX_LEN
        || !ticket
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("backup restore staging ticket is invalid".to_string());
    }
    Ok(())
}

fn validate_fingerprint(value: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err("backup restore fingerprint is invalid".to_string())
    }
}

fn random_ticket() -> Result<String, String> {
    let mut bytes = [0_u8; TICKET_BYTES];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("failed to create backup restore staging ticket: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn size_error() -> String {
    format!(
        "backup archive exceeds the {} MB safety limit",
        MAX_BACKUP_ARCHIVE_BYTES / 1024 / 1024
    )
}

#[cfg(unix)]
fn enforce_directory_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("failed to inspect backup staging permissions: {error}"))?
        .permissions();
    if permissions.mode() & 0o777 != 0o700 {
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("failed to secure backup staging directory: {error}"))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn enforce_directory_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn enforce_file_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("failed to inspect staged backup permissions: {error}"))?
        .permissions();
    if permissions.mode() & 0o777 != 0o600 {
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("failed to secure staged backup archive: {error}"))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn enforce_file_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("failed to sync backup staging directory: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "patina-backup-restore-staging-{label}-{}-{}",
            std::process::id(),
            crate::app::runtime::now_ms()
        ))
    }

    #[test]
    fn staged_archive_is_owner_only_and_requires_the_same_fingerprint() {
        let root = temp_root("valid");
        let source = root.with_extension("source.zip");
        fs::write(&source, b"backup archive").unwrap();

        let staged = stage_file(&root, &source).unwrap();
        let path = validate(&root, &staged.ticket, &staged.sha256, staged.size_bytes).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"backup archive");
        assert!(validate(&root, &staged.ticket, &"0".repeat(64), staged.size_bytes).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&root).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }

        discard(&root, &staged.ticket).unwrap();
        fs::remove_file(source).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discard_rejects_path_escape_and_only_removes_the_ticket_file() {
        let root = temp_root("discard");
        let source = root.with_extension("source.zip");
        fs::write(&source, b"backup archive").unwrap();
        let staged = stage_file(&root, &source).unwrap();

        assert!(discard(&root, "../backup").is_err());
        assert!(root.join(format!("{}.zip", staged.ticket)).is_file());
        discard(&root, &staged.ticket).unwrap();

        fs::remove_file(source).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn staging_rejects_symbolic_link_sources_and_roots() {
        use std::os::unix::fs::symlink;

        let outside = temp_root("outside");
        let source = outside.join("source.zip");
        let source_link = outside.join("source-link.zip");
        let root_link = temp_root("root-link");
        fs::create_dir_all(&outside).unwrap();
        fs::write(&source, b"backup archive").unwrap();
        symlink(&source, &source_link).unwrap();
        symlink(&outside, &root_link).unwrap();

        assert!(stage_file(&outside.join("staging"), &source_link).is_err());
        assert!(stage_file(&root_link, &source).is_err());

        fs::remove_file(root_link).unwrap();
        fs::remove_file(source_link).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
