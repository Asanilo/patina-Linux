use crate::data::repositories::activity_import;
use crate::data::sqlite_pool::wait_for_sqlite_pool;
use crate::domain::activity_import::{
    ImportBatchDto, ImportCommitReportDto, ImportDeleteReportDto, ImportPreviewDto,
    ImportPreviewErrorDto, ImportRecordType, MAX_IMPORT_FILE_BYTES, MAX_PREVIEW_ERRORS,
};
use crate::engine::activity_import::parse_canonical_csv;
use crate::engine::tracking::runtime as tracking_runtime;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime};

pub struct PreparedDaemonActivityImport {
    pub request: crate::engine::api::types::StagedActivityImportCommitRequest,
    staging_root: PathBuf,
}

impl PreparedDaemonActivityImport {
    pub fn discard(&self) -> Result<(), String> {
        crate::platform::activity_import_staging::discard(&self.staging_root, &self.request.ticket)
    }
}

struct LoadedCanonicalCsv {
    bytes: Vec<u8>,
    fingerprint: String,
    parsed: crate::domain::activity_import::ParsedCanonicalCsv,
}

pub async fn pick_canonical_csv_file(initial_path: Option<String>) -> Option<String> {
    let mut dialog = rfd::AsyncFileDialog::new().add_filter("Patina CSV", &["csv"]);
    if let Some(directory) = resolve_dialog_directory(initial_path) {
        dialog = dialog.set_directory(directory);
    }
    dialog
        .pick_file()
        .await
        .map(|file| file.path().to_string_lossy().to_string())
}

pub async fn preview<R: Runtime>(
    app: &AppHandle<R>,
    file_path: String,
) -> Result<ImportPreviewDto, String> {
    let path = validate_path(&file_path)?;
    let loaded = load_file(&path).await?;
    let pool = wait_for_sqlite_pool(app).await?;
    let mut known = activity_import::load_fingerprints(&pool).await?;
    let duplicate_records = loaded
        .parsed
        .records
        .iter()
        .filter(|record| !known.insert(crate::domain::activity_import::record_fingerprint(record)))
        .count();
    let exact_sessions = loaded
        .parsed
        .records
        .iter()
        .filter(|record| record.record_type == ImportRecordType::ExactSession)
        .count();
    let hour_buckets = loaded.parsed.records.len() - exact_sessions;

    Ok(ImportPreviewDto {
        file_path: path.to_string_lossy().to_string(),
        file_name: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Patina CSV")
            .to_string(),
        file_fingerprint: loaded.fingerprint,
        valid_records: loaded.parsed.records.len(),
        duplicate_records,
        error_records: loaded.parsed.errors.len(),
        exact_sessions,
        hour_buckets,
        errors: loaded
            .parsed
            .errors
            .into_iter()
            .take(MAX_PREVIEW_ERRORS)
            .map(|error| ImportPreviewErrorDto {
                line: error.line,
                message: error.message,
            })
            .collect(),
    })
}

pub async fn commit<R: Runtime>(
    app: AppHandle<R>,
    file_path: String,
    expected_fingerprint: String,
) -> Result<ImportCommitReportDto, String> {
    validate_preview_fingerprint(&expected_fingerprint)?;
    let path = validate_path(&file_path)?;
    let loaded = load_file(&path).await?;
    if loaded.fingerprint != expected_fingerprint {
        return Err("canonical CSV changed after preview; preview it again".to_string());
    }
    let source_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "canonical CSV file name is not valid UTF-8".to_string())?;
    let pool = wait_for_sqlite_pool(&app).await?;
    let report = activity_import::commit_records(
        &pool,
        source_name,
        &loaded.fingerprint,
        &loaded.parsed.records,
        loaded.parsed.errors.len(),
    )
    .await?;
    if report.imported_records > 0 {
        emit_refresh(&app, "external-data-imported");
    }
    Ok(report)
}

pub async fn stage_for_daemon<R: Runtime>(
    app: &AppHandle<R>,
    file_path: String,
    expected_fingerprint: String,
) -> Result<PreparedDaemonActivityImport, String> {
    validate_preview_fingerprint(&expected_fingerprint)?;
    let path = validate_path(&file_path)?;
    let loaded = load_file(&path).await?;
    if loaded.fingerprint != expected_fingerprint {
        return Err("canonical CSV changed after preview; preview it again".to_string());
    }
    let source_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "canonical CSV file name is not valid UTF-8".to_string())?
        .to_string();
    let storage_paths = crate::platform::storage_paths::resolve_storage_paths(app)?;
    let ticket = crate::platform::activity_import_staging::stage_bytes(
        &storage_paths.activity_import_staging_dir,
        &loaded.bytes,
    )?;

    Ok(PreparedDaemonActivityImport {
        request: crate::engine::api::types::StagedActivityImportCommitRequest {
            ticket,
            source_name,
            expected_fingerprint,
        },
        staging_root: storage_paths.activity_import_staging_dir,
    })
}

pub async fn list<R: Runtime>(app: &AppHandle<R>) -> Result<Vec<ImportBatchDto>, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    activity_import::list(&pool).await
}

pub async fn delete<R: Runtime>(
    app: AppHandle<R>,
    batch_id: String,
) -> Result<ImportDeleteReportDto, String> {
    let pool = wait_for_sqlite_pool(&app).await?;
    let report = activity_import::delete(&pool, batch_id.trim()).await?;
    emit_refresh(&app, "external-import-deleted");
    Ok(report)
}

async fn load_file(path: &Path) -> Result<LoadedCanonicalCsv, String> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| format!("failed to inspect canonical CSV: {error}"))?;
    if !metadata.is_file() {
        return Err("canonical import path must be a regular file".to_string());
    }
    if metadata.len() > MAX_IMPORT_FILE_BYTES {
        return Err(format!(
            "canonical CSV exceeds the {} MB safety limit",
            MAX_IMPORT_FILE_BYTES / 1024 / 1024
        ));
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| format!("failed to read canonical CSV: {error}"))?;
    if bytes.len() as u64 > MAX_IMPORT_FILE_BYTES {
        return Err(format!(
            "canonical CSV exceeds the {} MB safety limit",
            MAX_IMPORT_FILE_BYTES / 1024 / 1024
        ));
    }
    let fingerprint = format!("{:x}", Sha256::digest(&bytes));
    let parsed = parse_canonical_csv(&bytes)?;
    Ok(LoadedCanonicalCsv {
        bytes,
        fingerprint,
        parsed,
    })
}

fn validate_preview_fingerprint(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("preview fingerprint is required".to_string());
    }
    Ok(())
}

fn validate_path(file_path: &str) -> Result<PathBuf, String> {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return Err("canonical import path cannot be empty".to_string());
    }
    let path = PathBuf::from(trimmed);
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("csv"))
    {
        return Err("canonical import requires a CSV file".to_string());
    }
    Ok(path)
}

fn resolve_dialog_directory(initial_path: Option<String>) -> Option<PathBuf> {
    let path = PathBuf::from(initial_path?.trim());
    if path.is_dir() {
        Some(path)
    } else {
        path.parent()
            .filter(|parent| parent.is_dir())
            .map(Path::to_path_buf)
    }
}

fn emit_refresh<R: Runtime>(app: &AppHandle<R>, reason: &str) {
    if let Err(error) =
        tracking_runtime::emit_tracking_data_changed(app, reason, crate::app::runtime::now_ms())
    {
        eprintln!("[import] data committed but refresh event failed: {error}");
    }
}
