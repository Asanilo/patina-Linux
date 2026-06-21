use std::path::PathBuf;

pub fn send(
    app_id: &str,
    app_name: &str,
    title: &str,
    body: &str,
    icon_path: Option<PathBuf>,
) -> Result<(), String> {
    let mut notification = notify_rust::Notification::new();
    notification.appname(app_name).summary(title).body(body);

    if let Some(icon_path) = icon_path {
        if let Some(icon_str) = icon_path.to_str() {
            notification.icon(icon_str);
        }
    }

    notification
        .show()
        .map_err(|error| format!("failed to show notification: {error}"))?;

    Ok(())
}
