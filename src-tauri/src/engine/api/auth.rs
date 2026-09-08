use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::watch;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct ApiCredentialStore {
    inner: Arc<RwLock<Option<ApiCredentialState>>>,
    mutation: Arc<Mutex<()>>,
    revision_counter: Arc<AtomicU64>,
    revision_tx: watch::Sender<u64>,
}

#[derive(Clone, Debug)]
struct ApiCredentialState {
    token: String,
    path: PathBuf,
    revision: u64,
}

impl Default for ApiCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiCredentialStore {
    pub fn new() -> Self {
        let (revision_tx, _revision_rx) = watch::channel(0);
        Self {
            inner: Arc::new(RwLock::new(None)),
            mutation: Arc::new(Mutex::new(())),
            revision_counter: Arc::new(AtomicU64::new(0)),
            revision_tx,
        }
    }

    pub fn initialize_at(&self, path: &Path, legacy_token: Option<&str>) -> Result<String, String> {
        let _mutation = match self.mutation.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let token = initialize_token_file(path, legacy_token)?;
        self.replace_state(ApiCredentialState {
            token: token.clone(),
            path: path.to_path_buf(),
            revision: self.next_revision(),
        });
        Ok(token)
    }

    pub fn load_existing_at(&self, path: &Path) -> Result<String, String> {
        let _mutation = match self.mutation.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let token = load_token_file(path)?
            .ok_or_else(|| format!("patinad API credential is missing at `{}`", path.display()))?;
        self.replace_state(ApiCredentialState {
            token: token.clone(),
            path: path.to_path_buf(),
            revision: self.next_revision(),
        });
        Ok(token)
    }

    pub fn token(&self) -> Result<String, String> {
        self.with_state(|state| state.token.clone())
    }

    pub fn token_path(&self) -> Result<PathBuf, String> {
        self.with_state(|state| state.path.clone())
    }

    pub fn rotate(&self) -> Result<String, String> {
        let _mutation = match self.mutation.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let path = self.token_path()?;
        let token = generate_random_token()?;
        write_token_file_atomic(&path, &token)?;
        self.replace_state(ApiCredentialState {
            token: token.clone(),
            path,
            revision: self.next_revision(),
        });
        Ok(token)
    }

    pub fn validate(&self, authorization: Option<&str>) -> bool {
        self.validate_with_revision(authorization).is_some()
    }

    pub fn validate_with_revision(
        &self,
        authorization: Option<&str>,
    ) -> Option<watch::Receiver<u64>> {
        let auth_header = authorization?;
        let guard = match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let state = guard.as_ref()?;
        if auth_header.strip_prefix("Bearer ").unwrap_or(auth_header) != state.token {
            return None;
        }
        let receiver = self.revision_tx.subscribe();
        let matches_revision = *receiver.borrow() == state.revision;
        matches_revision.then_some(receiver)
    }

    fn with_state<T>(&self, read: impl FnOnce(&ApiCredentialState) -> T) -> Result<T, String> {
        let guard = match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard
            .as_ref()
            .map(read)
            .ok_or_else(|| "API credential store is not initialized".to_string())
    }

    fn replace_state(&self, state: ApiCredentialState) {
        let revision = state.revision;
        let mut guard = match self.inner.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard = Some(state);
        self.revision_tx.send_replace(revision);
    }

    fn next_revision(&self) -> u64 {
        self.revision_counter.fetch_add(1, Ordering::AcqRel) + 1
    }
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

fn initialize_token_file(path: &Path, legacy_token: Option<&str>) -> Result<String, String> {
    if let Some(token) = load_token_file(path)? {
        return Ok(token);
    }

    let legacy_token = legacy_token
        .map(str::trim)
        .filter(|token| !token.is_empty());
    let token = match legacy_token {
        Some(token) => token.to_string(),
        None => generate_random_token()?,
    };
    write_token_file_atomic(path, &token)?;
    Ok(token)
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

    static TEST_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_test_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "patina-api-auth-{label}-{}-{}",
            std::process::id(),
            TEST_PATH_COUNTER.fetch_add(1, Ordering::Relaxed)
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

    #[test]
    fn rotating_a_token_invalidates_existing_revision_subscriptions() {
        let path = unique_test_path("revision");
        let credentials = ApiCredentialStore::new();
        credentials
            .initialize_at(&path, Some("patina_api_old"))
            .unwrap();
        let revision = credentials
            .validate_with_revision(Some("Bearer patina_api_old"))
            .unwrap();

        let replacement = credentials.rotate().unwrap();

        assert!(revision.has_changed().unwrap());
        assert!(!credentials.validate(Some("Bearer patina_api_old")));
        assert!(credentials.validate(Some(&format!("Bearer {replacement}"))));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn concurrent_rotations_keep_memory_and_disk_credentials_consistent() {
        let path = unique_test_path("concurrent-rotation");
        let credentials = ApiCredentialStore::new();
        credentials
            .initialize_at(&path, Some("patina_api_old"))
            .unwrap();

        let rotated = std::thread::scope(|scope| {
            let first_store = credentials.clone();
            let second_store = credentials.clone();
            let first = scope.spawn(move || first_store.rotate().unwrap());
            let second = scope.spawn(move || second_store.rotate().unwrap());
            [first.join().unwrap(), second.join().unwrap()]
        });

        let current = credentials.token().unwrap();
        assert!(rotated.contains(&current));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), current);
        let _ = std::fs::remove_file(path);
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

    #[test]
    fn missing_token_file_is_seeded_from_legacy_storage_once() {
        let path = unique_test_path("legacy-seed");

        let token = initialize_token_file(&path, Some(" legacy-token ")).unwrap();

        assert_eq!(token, "legacy-token");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "legacy-token");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn existing_token_file_wins_over_legacy_storage() {
        let path = unique_test_path("file-wins");
        write_token_file_atomic(&path, "file-token").unwrap();

        let token = initialize_token_file(&path, Some("legacy-token")).unwrap();

        assert_eq!(token, "file-token");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn loading_existing_credentials_never_creates_a_missing_token() {
        let path = unique_test_path("read-only-missing");
        let credentials = ApiCredentialStore::new();

        let error = credentials.load_existing_at(&path).unwrap_err();

        assert!(error.contains("credential is missing"));
        assert!(!path.exists());
    }

    #[test]
    fn loading_existing_credentials_populates_the_read_only_client_store() {
        let path = unique_test_path("read-only-existing");
        write_token_file_atomic(&path, "patina_api_existing").unwrap();
        let credentials = ApiCredentialStore::new();

        let token = credentials.load_existing_at(&path).unwrap();

        assert_eq!(token, "patina_api_existing");
        assert_eq!(credentials.token_path().unwrap(), path);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn independent_credential_stores_never_cross_write() {
        let production_path = unique_test_path("store-production");
        let dev_path = unique_test_path("store-dev");
        let production = ApiCredentialStore::new();
        let dev = ApiCredentialStore::new();

        production
            .initialize_at(&production_path, Some("production-token"))
            .unwrap();
        dev.initialize_at(&dev_path, Some("dev-token")).unwrap();
        let rotated_dev = dev.rotate().unwrap();

        assert_eq!(production.token().unwrap(), "production-token");
        assert_eq!(
            std::fs::read_to_string(&production_path).unwrap(),
            "production-token"
        );
        assert_eq!(std::fs::read_to_string(&dev_path).unwrap(), rotated_dev);
        assert_ne!(rotated_dev, "production-token");
        let _ = std::fs::remove_file(production_path);
        let _ = std::fs::remove_file(dev_path);
    }

    #[test]
    fn rotation_reuses_the_active_token_path() {
        let path = unique_test_path("store-rotation");
        let store = ApiCredentialStore::new();
        let initial = store.initialize_at(&path, None).unwrap();

        let rotated = store.rotate().unwrap();

        assert_ne!(initial, rotated);
        assert_eq!(store.token_path().unwrap(), path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), rotated);
        let _ = std::fs::remove_file(path);
    }
}
