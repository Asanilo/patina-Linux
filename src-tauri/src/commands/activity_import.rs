use crate::app::activity_import;
use crate::domain::activity_import::{
    ImportBatchDto, ImportCommitReportDto, ImportDeleteReportDto, ImportPreviewDto,
};
use tauri::AppHandle;

#[tauri::command]
pub fn cmd_pick_activity_import_file(initial_path: Option<String>) -> Option<String> {
    activity_import::pick_canonical_csv_file(initial_path)
}

#[tauri::command]
pub async fn cmd_preview_activity_import(
    file_path: String,
    app: AppHandle,
) -> Result<ImportPreviewDto, String> {
    activity_import::preview(&app, file_path).await
}

#[tauri::command]
pub async fn cmd_commit_activity_import(
    file_path: String,
    expected_fingerprint: String,
    app: AppHandle,
) -> Result<ImportCommitReportDto, String> {
    if crate::app::daemon_client::command_client(&app)?.is_some() {
        return Err(
            "activity import writes are not available in daemon client preview yet".to_string(),
        );
    }
    activity_import::commit(app, file_path, expected_fingerprint).await
}

#[tauri::command]
pub async fn cmd_list_activity_import_batches(
    app: AppHandle,
) -> Result<Vec<ImportBatchDto>, String> {
    activity_import::list(&app).await
}

#[tauri::command]
pub async fn cmd_delete_activity_import_batch(
    batch_id: String,
    app: AppHandle,
) -> Result<ImportDeleteReportDto, String> {
    if crate::app::daemon_client::command_client(&app)?.is_some() {
        return Err(
            "activity import deletion is not available in daemon client preview yet".to_string(),
        );
    }
    activity_import::delete(app, batch_id).await
}
