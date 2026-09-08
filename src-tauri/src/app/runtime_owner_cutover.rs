use crate::platform::app_paths::AppProfile;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const CUTOVER_FILE_NAME: &str = "runtime-owner-cutover.json";
const CUTOVER_VERSION: u32 = 1;
const MAX_CUTOVER_BYTES: u64 = 32 * 1024;
const MAX_FAILURE_CODE_BYTES: usize = 64;
const MAX_FAILURE_MESSAGE_BYTES: usize = 512;
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOwnerCutoverStatus {
    Prepared,
    Activating,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RuntimeOwnerCutoverReservation {
    version: u32,
    request_id: String,
    profile: String,
    status: RuntimeOwnerCutoverStatus,
    requested_at_ms: u64,
    updated_at_ms: u64,
    requested_desktop_pid: u32,
    background_tracking_at_login: bool,
    desktop_launch_at_login: bool,
    failure_code: Option<String>,
    failure_message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeOwnerCutoverSnapshot {
    pub request_id: String,
    pub profile: String,
    pub status: RuntimeOwnerCutoverStatus,
    pub requested_at_ms: u64,
    pub updated_at_ms: u64,
    pub background_tracking_at_login: bool,
    pub desktop_launch_at_login: bool,
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeOwnerStartupDecision {
    Embedded,
    DaemonClient {
        reservation: RuntimeOwnerCutoverSnapshot,
        should_attempt_service_start: bool,
    },
    Blocked {
        reason: String,
    },
}

impl RuntimeOwnerStartupDecision {
    pub const fn owns_embedded_runtime(&self) -> bool {
        matches!(self, Self::Embedded)
    }
}

pub fn decide_desktop_startup(
    control_root: &Path,
    profile: AppProfile,
) -> RuntimeOwnerStartupDecision {
    let reservation = match read_reservation(control_root, profile) {
        Ok(Some(reservation)) => reservation,
        Ok(None) => return RuntimeOwnerStartupDecision::Embedded,
        Err(reason) => return RuntimeOwnerStartupDecision::Blocked { reason },
    };
    let should_attempt_service_start = matches!(
        reservation.status,
        RuntimeOwnerCutoverStatus::Prepared
            | RuntimeOwnerCutoverStatus::Activating
            | RuntimeOwnerCutoverStatus::Completed
    );
    RuntimeOwnerStartupDecision::DaemonClient {
        reservation: snapshot(&reservation),
        should_attempt_service_start,
    }
}

pub fn prepare(
    control_root: &Path,
    profile: AppProfile,
    background_tracking_at_login: bool,
    desktop_launch_at_login: bool,
    now_ms: u64,
) -> Result<RuntimeOwnerCutoverSnapshot, String> {
    if let Some(existing) = read_reservation(control_root, profile)? {
        return Ok(snapshot(&existing));
    }
    let reservation = RuntimeOwnerCutoverReservation {
        version: CUTOVER_VERSION,
        request_id: random_request_id()?,
        profile: profile.key().to_string(),
        status: RuntimeOwnerCutoverStatus::Prepared,
        requested_at_ms: now_ms,
        updated_at_ms: now_ms,
        requested_desktop_pid: std::process::id(),
        background_tracking_at_login,
        desktop_launch_at_login,
        failure_code: None,
        failure_message: None,
    };
    write_reservation_atomic(control_root, &reservation)?;
    Ok(snapshot(&reservation))
}

pub fn mark_activating(
    control_root: &Path,
    profile: AppProfile,
    request_id: &str,
    now_ms: u64,
) -> Result<RuntimeOwnerCutoverSnapshot, String> {
    update_reservation(control_root, profile, request_id, |reservation| {
        match reservation.status {
            RuntimeOwnerCutoverStatus::Prepared => {
                reservation.status = RuntimeOwnerCutoverStatus::Activating;
                reservation.updated_at_ms = reservation.updated_at_ms.max(now_ms);
            }
            RuntimeOwnerCutoverStatus::Activating => {}
            RuntimeOwnerCutoverStatus::Completed => {
                return Err("runtime owner cutover is already completed".to_string())
            }
            RuntimeOwnerCutoverStatus::Failed => {
                return Err("failed runtime owner cutover requires explicit repair".to_string())
            }
        }
        Ok(())
    })
}

pub fn mark_completed(
    control_root: &Path,
    profile: AppProfile,
    request_id: &str,
    now_ms: u64,
) -> Result<RuntimeOwnerCutoverSnapshot, String> {
    update_reservation(control_root, profile, request_id, |reservation| {
        match reservation.status {
            RuntimeOwnerCutoverStatus::Activating => {
                reservation.status = RuntimeOwnerCutoverStatus::Completed;
                reservation.updated_at_ms = reservation.updated_at_ms.max(now_ms);
                reservation.failure_code = None;
                reservation.failure_message = None;
            }
            RuntimeOwnerCutoverStatus::Completed => {}
            RuntimeOwnerCutoverStatus::Prepared => {
                return Err("runtime owner cutover has not started activation".to_string())
            }
            RuntimeOwnerCutoverStatus::Failed => {
                return Err("failed runtime owner cutover cannot be completed".to_string())
            }
        }
        Ok(())
    })
}

pub fn mark_failed(
    control_root: &Path,
    profile: AppProfile,
    request_id: &str,
    failure_code: &str,
    failure_message: &str,
    now_ms: u64,
) -> Result<RuntimeOwnerCutoverSnapshot, String> {
    let failure_message = bounded_failure_message(failure_message);
    validate_failure(failure_code, &failure_message)?;
    update_reservation(control_root, profile, request_id, |reservation| {
        match reservation.status {
            RuntimeOwnerCutoverStatus::Prepared | RuntimeOwnerCutoverStatus::Activating => {
                reservation.status = RuntimeOwnerCutoverStatus::Failed;
                reservation.updated_at_ms = reservation.updated_at_ms.max(now_ms);
                reservation.failure_code = Some(failure_code.to_string());
                reservation.failure_message = Some(failure_message);
            }
            RuntimeOwnerCutoverStatus::Failed => {}
            RuntimeOwnerCutoverStatus::Completed => {
                return Err("completed runtime owner cutover cannot be failed".to_string())
            }
        }
        Ok(())
    })
}

fn update_reservation(
    control_root: &Path,
    profile: AppProfile,
    request_id: &str,
    update: impl FnOnce(&mut RuntimeOwnerCutoverReservation) -> Result<(), String>,
) -> Result<RuntimeOwnerCutoverSnapshot, String> {
    let mut reservation = read_reservation(control_root, profile)?
        .ok_or_else(|| "runtime owner cutover reservation does not exist".to_string())?;
    if reservation.request_id != request_id {
        return Err("runtime owner cutover request ID does not match".to_string());
    }
    let before = reservation.clone();
    update(&mut reservation)?;
    validate_reservation(&reservation, profile)?;
    if reservation != before {
        write_reservation_atomic(control_root, &reservation)?;
    }
    Ok(snapshot(&reservation))
}

fn read_reservation(
    control_root: &Path,
    profile: AppProfile,
) -> Result<Option<RuntimeOwnerCutoverReservation>, String> {
    let path = reservation_path(control_root);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to inspect runtime owner cutover: {error}")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("runtime owner cutover must be a regular non-symlink file".to_string());
    }
    if metadata.len() > MAX_CUTOVER_BYTES {
        return Err("runtime owner cutover exceeds the size limit".to_string());
    }
    require_owner_only_file(&path)?;
    let raw = fs::read(&path)
        .map_err(|error| format!("failed to read runtime owner cutover: {error}"))?;
    let reservation = serde_json::from_slice::<RuntimeOwnerCutoverReservation>(&raw)
        .map_err(|error| format!("failed to parse runtime owner cutover: {error}"))?;
    validate_reservation(&reservation, profile)?;
    Ok(Some(reservation))
}

fn validate_reservation(
    reservation: &RuntimeOwnerCutoverReservation,
    profile: AppProfile,
) -> Result<(), String> {
    if reservation.version != CUTOVER_VERSION {
        return Err(format!(
            "unsupported runtime owner cutover version {}",
            reservation.version
        ));
    }
    if reservation.profile != profile.key() {
        return Err(format!(
            "runtime owner cutover profile `{}` does not match `{}`",
            reservation.profile,
            profile.key()
        ));
    }
    if !valid_request_id(&reservation.request_id) {
        return Err("runtime owner cutover request ID is invalid".to_string());
    }
    if reservation.updated_at_ms < reservation.requested_at_ms {
        return Err("runtime owner cutover timestamp order is invalid".to_string());
    }
    if reservation.requested_desktop_pid == 0 {
        return Err("runtime owner cutover desktop PID is invalid".to_string());
    }
    match reservation.status {
        RuntimeOwnerCutoverStatus::Failed => {
            let code = reservation
                .failure_code
                .as_deref()
                .ok_or_else(|| "failed runtime owner cutover has no failure code".to_string())?;
            let message = reservation
                .failure_message
                .as_deref()
                .ok_or_else(|| "failed runtime owner cutover has no failure message".to_string())?;
            validate_failure(code, message)?;
        }
        _ if reservation.failure_code.is_some() || reservation.failure_message.is_some() => {
            return Err("non-failed runtime owner cutover contains failure details".to_string())
        }
        _ => {}
    }
    Ok(())
}

fn validate_failure(code: &str, message: &str) -> Result<(), String> {
    if code.is_empty()
        || code.len() > MAX_FAILURE_CODE_BYTES
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("runtime owner cutover failure code is invalid".to_string());
    }
    if message.trim().is_empty() || message.len() > MAX_FAILURE_MESSAGE_BYTES {
        return Err("runtime owner cutover failure message is invalid".to_string());
    }
    Ok(())
}

fn bounded_failure_message(message: &str) -> String {
    let message = message.trim();
    if message.len() <= MAX_FAILURE_MESSAGE_BYTES {
        return message.to_string();
    }
    let mut end = MAX_FAILURE_MESSAGE_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_string()
}

fn valid_request_id(request_id: &str) -> bool {
    request_id.strip_prefix("cutover_").is_some_and(|payload| {
        payload.len() == 32 && payload.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn random_request_id() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("failed to generate runtime owner cutover ID: {error}"))?;
    Ok(format!(
        "cutover_{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

fn snapshot(reservation: &RuntimeOwnerCutoverReservation) -> RuntimeOwnerCutoverSnapshot {
    RuntimeOwnerCutoverSnapshot {
        request_id: reservation.request_id.clone(),
        profile: reservation.profile.clone(),
        status: reservation.status,
        requested_at_ms: reservation.requested_at_ms,
        updated_at_ms: reservation.updated_at_ms,
        background_tracking_at_login: reservation.background_tracking_at_login,
        desktop_launch_at_login: reservation.desktop_launch_at_login,
        failure_code: reservation.failure_code.clone(),
        failure_message: reservation.failure_message.clone(),
    }
}

fn reservation_path(control_root: &Path) -> PathBuf {
    control_root.join(CUTOVER_FILE_NAME)
}

fn write_reservation_atomic(
    control_root: &Path,
    reservation: &RuntimeOwnerCutoverReservation,
) -> Result<(), String> {
    fs::create_dir_all(control_root)
        .map_err(|error| format!("failed to create runtime control directory: {error}"))?;
    let root_metadata = fs::symlink_metadata(control_root)
        .map_err(|error| format!("failed to inspect runtime control directory: {error}"))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("runtime control root must be a real directory".to_string());
    }
    validate_reservation(
        reservation,
        profile_from_key(&reservation.profile)
            .ok_or_else(|| "runtime owner cutover profile is invalid".to_string())?,
    )?;
    let path = reservation_path(control_root);
    match fs::symlink_metadata(&path) {
        Ok(_) => require_owner_only_file(&path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to inspect existing runtime owner cutover: {error}"
            ))
        }
    }
    let temporary_path = control_root.join(format!(
        ".{CUTOVER_FILE_NAME}.tmp-{}-{}",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<(), String> {
        let mut file = options.open(&temporary_path).map_err(|error| {
            format!("failed to create runtime owner cutover temporary file: {error}")
        })?;
        let bytes = serde_json::to_vec(reservation)
            .map_err(|error| format!("failed to serialize runtime owner cutover: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("failed to write runtime owner cutover: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync runtime owner cutover: {error}"))?;
        drop(file);
        fs::rename(&temporary_path, &path)
            .map_err(|error| format!("failed to replace runtime owner cutover: {error}"))?;
        require_owner_only_file(&path)?;
        File::open(control_root)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("failed to sync runtime control directory: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn profile_from_key(key: &str) -> Option<AppProfile> {
    match key {
        "production" => Some(AppProfile::Production),
        "local" => Some(AppProfile::Local),
        "dev" => Some(AppProfile::Dev),
        _ => None,
    }
}

#[cfg(unix)]
fn require_owner_only_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect runtime owner cutover: {error}"))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err("runtime owner cutover must be an owner-only 0600 file".to_string());
    }
    Ok(())
}

#[cfg(not(unix))]
fn require_owner_only_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect runtime owner cutover: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("runtime owner cutover must be a regular non-symlink file".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "patina-runtime-owner-cutover-{label}-{}-{}",
            std::process::id(),
            TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn missing_reservation_keeps_the_embedded_owner() {
        let root = root("missing");

        let decision = decide_desktop_startup(&root, AppProfile::Dev);

        assert_eq!(decision, RuntimeOwnerStartupDecision::Embedded);
        assert!(decision.owns_embedded_runtime());
    }

    #[test]
    fn prepare_is_idempotent_and_creates_an_owner_only_marker() {
        let root = root("prepare");

        let first = prepare(&root, AppProfile::Dev, true, false, 1_000).unwrap();
        let second = prepare(&root, AppProfile::Dev, false, true, 2_000).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.status, RuntimeOwnerCutoverStatus::Prepared);
        assert!(matches!(
            decide_desktop_startup(&root, AppProfile::Dev),
            RuntimeOwnerStartupDecision::DaemonClient {
                should_attempt_service_start: true,
                ..
            }
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(reservation_path(&root))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn activation_and_completion_are_durable_and_idempotent() {
        let root = root("complete");
        let prepared = prepare(&root, AppProfile::Production, true, true, 1_000).unwrap();

        let activating =
            mark_activating(&root, AppProfile::Production, &prepared.request_id, 2_000).unwrap();
        let activating_again =
            mark_activating(&root, AppProfile::Production, &prepared.request_id, 3_000).unwrap();
        assert_eq!(activating, activating_again);
        let completed =
            mark_completed(&root, AppProfile::Production, &prepared.request_id, 4_000).unwrap();
        let completed_again =
            mark_completed(&root, AppProfile::Production, &prepared.request_id, 5_000).unwrap();

        assert_eq!(completed, completed_again);
        assert_eq!(completed.status, RuntimeOwnerCutoverStatus::Completed);
        assert_eq!(completed.updated_at_ms, 4_000);
        assert!(matches!(
            decide_desktop_startup(&root, AppProfile::Production),
            RuntimeOwnerStartupDecision::DaemonClient {
                should_attempt_service_start: true,
                ..
            }
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_cutover_never_falls_back_to_embedded_or_retries_implicitly() {
        let root = root("failed");
        let prepared = prepare(&root, AppProfile::Production, true, true, 1_000).unwrap();

        let failed = mark_failed(
            &root,
            AppProfile::Production,
            &prepared.request_id,
            "service-start-failed",
            "patinad.service entered failed state",
            2_000,
        )
        .unwrap();

        assert_eq!(failed.status, RuntimeOwnerCutoverStatus::Failed);
        assert!(matches!(
            decide_desktop_startup(&root, AppProfile::Production),
            RuntimeOwnerStartupDecision::DaemonClient {
                should_attempt_service_start: false,
                ..
            }
        ));
        assert!(!decide_desktop_startup(&root, AppProfile::Production).owns_embedded_runtime());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wrong_request_id_cannot_advance_the_reservation() {
        let root = root("wrong-id");
        let prepared = prepare(&root, AppProfile::Dev, true, true, 1_000).unwrap();

        let error = mark_activating(
            &root,
            AppProfile::Dev,
            "cutover_00000000000000000000000000000000",
            2_000,
        )
        .unwrap_err();

        assert!(error.contains("does not match"));
        assert_eq!(
            prepare(&root, AppProfile::Dev, false, false, 3_000)
                .unwrap()
                .status,
            RuntimeOwnerCutoverStatus::Prepared
        );
        assert_ne!(
            prepared.request_id,
            "cutover_00000000000000000000000000000000"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn profile_mismatch_and_invalid_permissions_fail_closed() {
        let root = root("untrusted");
        prepare(&root, AppProfile::Production, true, true, 1_000).unwrap();

        assert!(matches!(
            decide_desktop_startup(&root, AppProfile::Dev),
            RuntimeOwnerStartupDecision::Blocked { .. }
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = reservation_path(&root);
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(matches!(
                decide_desktop_startup(&root, AppProfile::Production),
                RuntimeOwnerStartupDecision::Blocked { .. }
            ));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failure_messages_are_bounded_without_splitting_utf8() {
        let root = root("bounded-failure");
        let prepared = prepare(&root, AppProfile::Dev, true, true, 1_000).unwrap();
        let message = "错".repeat(300);

        let failed = mark_failed(
            &root,
            AppProfile::Dev,
            &prepared.request_id,
            "activation-failed",
            &message,
            2_000,
        )
        .unwrap();

        let stored = failed.failure_message.unwrap();
        assert!(stored.len() <= MAX_FAILURE_MESSAGE_BYTES);
        assert!(message.starts_with(&stored));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlink_reservation_fails_closed() {
        use std::os::unix::fs::symlink;

        let root = root("symlink");
        fs::create_dir_all(&root).unwrap();
        let target = root.join("target.json");
        fs::write(&target, b"{}").unwrap();
        symlink(&target, reservation_path(&root)).unwrap();

        assert!(matches!(
            decide_desktop_startup(&root, AppProfile::Dev),
            RuntimeOwnerStartupDecision::Blocked { .. }
        ));
        fs::remove_dir_all(root).unwrap();
    }
}
