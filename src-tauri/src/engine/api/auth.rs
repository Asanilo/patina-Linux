use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

static API_TOKEN: OnceLock<RwLock<String>> = OnceLock::new();
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[allow(dead_code)]
pub fn set_api_token(token: String) {
    let lock = API_TOKEN.get_or_init(|| RwLock::new(load_or_generate_token()));
    match lock.write() {
        Ok(mut guard) => {
            *guard = token;
        }
        Err(poisoned) => {
            *poisoned.into_inner() = token;
        }
    }
}

pub fn get_api_token() -> String {
    let lock = API_TOKEN.get_or_init(|| RwLock::new(load_or_generate_token()));
    match lock.read() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

pub fn replace_api_token(token: &str) -> Result<String, String> {
    let normalized = token.trim().to_string();
    if normalized.is_empty() {
        return Err("API token cannot be empty".to_string());
    }

    write_token_file_atomic(&token_file_path(), &normalized)?;
    set_api_token(normalized.clone());
    Ok(normalized)
}

pub fn token_file_path() -> std::path::PathBuf {
    let data_dir = std::env::var("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            std::path::PathBuf::from(home).join(".local/share")
        });
    data_dir.join("Patina").join("api_token")
}

fn load_or_generate_token() -> String {
    let path = token_file_path();
    match load_token_file(&path) {
        Ok(Some(token)) => return token,
        Ok(None) => {}
        Err(error) => eprintln!("[api] failed to load token file: {error}"),
    }

    let token = generate_random_token()
        .unwrap_or_else(|error| panic!("failed to generate local API token: {error}"));
    if let Err(error) = write_token_file_atomic(&path, &token) {
        eprintln!("[api] failed to persist generated token: {error}");
    }
    token
}

pub fn validate_token(authorization: Option<&str>) -> bool {
    let expected = get_api_token();
    let Some(auth_header) = authorization else {
        return false;
    };

    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);

    token == expected
}

fn generate_random_token() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("failed to read operating system random source: {error}"))?;
    let payload = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("patina_api_{payload}"))
}

fn load_token_file(path: &Path) -> Result<Option<String>, String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to read API token file: {error}")),
    };

    enforce_owner_only_permissions(path)?;
    let token = raw.trim().to_string();
    if token.is_empty() {
        return Ok(None);
    }
    Ok(Some(token))
}

fn write_token_file_atomic(path: &Path, token: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "API token path has no parent directory".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create API token directory: {error}"))?;

    let (temporary_path, mut file) = create_temporary_token_file(path)?;
    let write_result = (|| -> Result<(), String> {
        file.write_all(token.as_bytes())
            .map_err(|error| format!("failed to write temporary API token file: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync temporary API token file: {error}"))?;
        drop(file);
        std::fs::rename(&temporary_path, path)
            .map_err(|error| format!("failed to replace API token file: {error}"))?;
        enforce_owner_only_permissions(path)?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    write_result
}

fn create_temporary_token_file(path: &Path) -> Result<(PathBuf, std::fs::File), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "API token path has no parent directory".to_string())?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("api_token");

    for _ in 0..16 {
        let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary_path = parent.join(format!(
            ".{file_name}.tmp-{}-{sequence}",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        match options.open(&temporary_path) {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "failed to create temporary API token file: {error}"
                ));
            }
        }
    }

    Err("failed to allocate a temporary API token file".to_string())
}

#[cfg(unix)]
fn enforce_owner_only_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
        .map_err(|error| format!("failed to inspect API token permissions: {error}"))?
        .permissions();
    if permissions.mode() & 0o777 != 0o600 {
        permissions.set_mode(0o600);
        std::fs::set_permissions(path, permissions)
            .map_err(|error| format!("failed to secure API token permissions: {error}"))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn enforce_owner_only_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "patina-api-auth-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn generated_tokens_use_a_256_bit_lowercase_hex_payload() {
        let token = generate_random_token().unwrap();
        let payload = token.strip_prefix("patina_api_").unwrap();

        assert_eq!(payload.len(), 64);
        assert!(payload
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()));
    }

    #[test]
    fn generated_tokens_are_not_reused() {
        assert_ne!(
            generate_random_token().unwrap(),
            generate_random_token().unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_token_write_enforces_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let path = unique_test_path("create-mode");
        write_token_file_atomic(&path, "patina_api_test").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn loading_a_token_repairs_broad_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let path = unique_test_path("repair-mode");
        std::fs::write(&path, "patina_api_existing\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o664)).unwrap();

        assert_eq!(
            load_token_file(&path).unwrap().as_deref(),
            Some("patina_api_existing")
        );
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_file(path);
    }
}
