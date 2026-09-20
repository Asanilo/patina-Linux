use crate::platform::storage_anchor::{self, PendingStorageMigration};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const VERSION: u32 = 1;
const MAX_BYTES: u64 = 128 * 1024;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum State {
    Active,
    Committed,
    RolledBack,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PromotionPhase {
    Quarantining,
    Promoting,
    Promoted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PromotionKind {
    Data,
    Webview,
}

impl PromotionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Webview => "webview",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Promotion {
    pub kind: PromotionKind,
    pub target_root: PathBuf,
    pub staging_root: PathBuf,
    pub quarantine_root: Option<PathBuf>,
    pub target_created: bool,
    pub entries: Vec<String>,
    pub original_entries: Vec<String>,
    pub phase: PromotionPhase,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Journal {
    version: u32,
    pub pending: PendingStorageMigration,
    pub state: State,
    pub promotions: Vec<Promotion>,
}

impl Journal {
    pub fn new(pending: &PendingStorageMigration) -> Self {
        Self {
            version: VERSION,
            pending: pending.clone(),
            state: State::Active,
            promotions: Vec::new(),
        }
    }
}

pub(super) fn read(control_root: &Path) -> Result<Option<Journal>, String> {
    let path = storage_anchor::storage_migration_journal_path(control_root);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "failed to inspect storage migration journal: {error}"
            ))
        }
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_BYTES {
        return Err("storage migration journal is not a bounded regular file".to_string());
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read storage migration journal: {error}"))?;
    let journal: Journal = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse storage migration journal: {error}"))?;
    if journal.version != VERSION {
        return Err("unsupported storage migration journal version".to_string());
    }
    Ok(Some(journal))
}

pub(super) fn write(control_root: &Path, journal: &Journal) -> Result<(), String> {
    let path = storage_anchor::storage_migration_journal_path(control_root);
    fs::create_dir_all(control_root).map_err(|error| {
        format!("failed to create storage migration control directory: {error}")
    })?;
    let temporary = control_root.join(format!(
        ".storage-migration-journal.{}-{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options
            .open(&temporary)
            .map_err(|error| format!("failed to create storage migration journal: {error}"))?;
        let bytes = serde_json::to_vec(journal)
            .map_err(|error| format!("failed to encode storage migration journal: {error}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("failed to persist storage migration journal: {error}"))?;
        drop(file);
        fs::rename(&temporary, path)
            .map_err(|error| format!("failed to replace storage migration journal: {error}"))?;
        sync_directory(control_root)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub(super) fn remove(control_root: &Path) -> Result<(), String> {
    match fs::remove_file(storage_anchor::storage_migration_journal_path(control_root)) {
        Ok(()) => sync_directory(control_root),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "failed to remove storage migration journal: {error}"
        )),
    }
}

fn sync_directory(path: &Path) -> Result<(), String> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("failed to sync storage migration control directory: {error}"))
}
