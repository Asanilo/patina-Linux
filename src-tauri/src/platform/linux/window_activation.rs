use tauri::{Runtime, WebviewWindow};

pub(crate) fn restore_to_foreground<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    // On Linux with Tauri, we can use the Tauri window API directly
    window
        .show()
        .map_err(|e| format!("failed to show window: {e}"))?;
    window
        .set_focus()
        .map_err(|e| format!("failed to set window focus: {e}"))?;
    window
        .unminimize()
        .map_err(|e| format!("failed to unminimize window: {e}"))?;
    Ok(())
}
