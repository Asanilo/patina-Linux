use crate::domain::activity_import::MAX_IMPORT_FILE_BYTES;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const TICKET_BYTES: usize = 16;
const TICKET_HEX_LEN: usize = TICKET_BYTES * 2;
static CLAIM_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn stage_bytes(root: &Path, bytes: &[u8]) -> Result<String, String> {
    if bytes.len() as u64 > MAX_IMPORT_FILE_BYTES {
        return Err(import_size_error());
    }
    prepare_root(root)?;

    for _ in 0..16 {
        let ticket = random_ticket()?;
        let path = staged_path(root, &ticket)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(mut file) => {
                let result = (|| -> Result<(), String> {
                    file.write_all(bytes).map_err(|error| {
                        format!("failed to write staged activity import: {error}")
                    })?;
                    file.sync_all().map_err(|error| {
                        format!("failed to sync staged activity import: {error}")
                    })?;
                    enforce_file_permissions(&path)?;
                    Ok(())
                })();
                if result.is_err() {
                    let _ = fs::remove_file(&path);
                }
                result?;
                return Ok(ticket);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("failed to create staged activity import: {error}"));
            }
        }
    }

    Err("failed to allocate an activity import staging ticket".to_string())
}

pub fn consume_bytes(root: &Path, ticket: &str) -> Result<Vec<u8>, String> {
    validate_existing_root(root)?;
    let source = staged_path(root, ticket)?;
    validate_staged_file(&source)?;
    let claimed = claimed_path(root, ticket);
    fs::rename(&source, &claimed)
        .map_err(|error| format!("failed to claim staged activity import: {error}"))?;

    let result = (|| -> Result<Vec<u8>, String> {
        validate_staged_file(&claimed)?;
        let bytes = fs::read(&claimed)
            .map_err(|error| format!("failed to read staged activity import: {error}"))?;
        if bytes.len() as u64 > MAX_IMPORT_FILE_BYTES {
            return Err(import_size_error());
        }
        Ok(bytes)
    })();
    let remove_result = fs::remove_file(&claimed)
        .map_err(|error| format!("failed to remove consumed activity import: {error}"));

    match (result, remove_result) {
        (Ok(bytes), Ok(())) => Ok(bytes),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

pub fn discard(root: &Path, ticket: &str) -> Result<(), String> {
    validate_ticket(ticket)?;
    match fs::symlink_metadata(root) {
        Ok(_) => validate_existing_root(root)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "activity import staging directory is unavailable: {error}"
            ));
        }
    }
    let path = root.join(format!("{ticket}.csv"));
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to discard staged activity import: {error}")),
    }
}

fn prepare_root(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root)
        .map_err(|error| format!("failed to create activity import staging directory: {error}"))?;
    validate_existing_root(root)?;
    enforce_directory_permissions(root)
}

fn validate_existing_root(root: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("activity import staging directory is unavailable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("activity import staging path must be a real directory".to_string());
    }
    Ok(())
}

fn validate_staged_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("staged activity import is unavailable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("staged activity import must be a regular file".to_string());
    }
    if metadata.len() > MAX_IMPORT_FILE_BYTES {
        return Err(import_size_error());
    }
    enforce_file_permissions(path)
}

fn staged_path(root: &Path, ticket: &str) -> Result<PathBuf, String> {
    validate_ticket(ticket)?;
    Ok(root.join(format!("{ticket}.csv")))
}

fn claimed_path(root: &Path, ticket: &str) -> PathBuf {
    let sequence = CLAIM_COUNTER.fetch_add(1, Ordering::Relaxed);
    root.join(format!(
        ".{ticket}.processing-{}-{sequence}",
        std::process::id()
    ))
}

fn validate_ticket(ticket: &str) -> Result<(), String> {
    if ticket.len() != TICKET_HEX_LEN
        || !ticket
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("activity import staging ticket is invalid".to_string());
    }
    Ok(())
}

fn random_ticket() -> Result<String, String> {
    let mut bytes = [0_u8; TICKET_BYTES];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("failed to create activity import staging ticket: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn import_size_error() -> String {
    format!(
        "canonical CSV exceeds the {} MB safety limit",
        MAX_IMPORT_FILE_BYTES / 1024 / 1024
    )
}

#[cfg(unix)]
fn enforce_directory_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("failed to inspect activity import staging permissions: {error}"))?
        .permissions();
    if permissions.mode() & 0o777 != 0o700 {
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions).map_err(|error| {
            format!("failed to secure activity import staging directory: {error}")
        })?;
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
        .map_err(|error| format!("failed to inspect staged activity import permissions: {error}"))?
        .permissions();
    if permissions.mode() & 0o777 != 0o600 {
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions)
            .map_err(|error| format!("failed to secure staged activity import: {error}"))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn enforce_file_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "patina-import-staging-{label}-{}-{}",
            std::process::id(),
            crate::app::runtime::now_ms()
        ))
    }

    #[test]
    fn staged_bytes_are_owner_only_and_consumed_once() {
        let root = temp_root("consume");
        let ticket = stage_bytes(&root, b"record_type\n").unwrap();
        let path = root.join(format!("{ticket}.csv"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&root).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(consume_bytes(&root, &ticket).unwrap(), b"record_type\n");
        assert!(!path.exists());
        assert!(consume_bytes(&root, &ticket).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ticket_validation_prevents_path_escape_and_discard_is_exact() {
        let root = temp_root("escape");
        fs::create_dir_all(&root).unwrap();
        let outside = root.parent().unwrap().join("do-not-delete.csv");
        fs::write(&outside, b"keep").unwrap();

        assert!(discard(&root, "../do-not-delete").is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"keep");

        fs::remove_file(outside).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn staging_rejects_a_symbolic_link_root() {
        use std::os::unix::fs::symlink;

        let outside = temp_root("outside");
        let link = temp_root("link");
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, &link).unwrap();

        assert!(stage_bytes(&link, b"data").is_err());
        assert!(fs::read_dir(&outside).unwrap().next().is_none());

        fs::remove_file(link).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
