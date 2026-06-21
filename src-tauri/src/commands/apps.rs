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
