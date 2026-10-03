#[tauri::command]
pub fn get_icon(exe_path: String) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        crate::platform::windows::icon::get_icon_base64(&exe_path)
    }
    #[cfg(target_os = "linux")]
    {
        crate::platform::linux::icon::get_icon_base64(&exe_path)
    }
}

#[tauri::command]
pub async fn cmd_get_cached_icon_page<R: tauri::Runtime>(
    after: Option<String>,
    limit: usize,
    app: tauri::AppHandle<R>,
) -> Result<patina_protocol::icons::IconPage, String> {
    crate::data::icon_cache_service::load_icon_page(&app, after.as_deref(), limit).await
}
