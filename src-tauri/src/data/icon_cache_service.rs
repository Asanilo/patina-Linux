use crate::data::repositories::icon_cache::read;
use crate::data::sqlite_pool::wait_for_sqlite_pool;
use patina_protocol::icons::IconPage;
use tauri::{AppHandle, Runtime};

pub async fn load_icon_page<R: Runtime>(
    app: &AppHandle<R>,
    after: Option<&str>,
    limit: usize,
) -> Result<IconPage, String> {
    if let Some(client) = crate::app::daemon_client::command_client(app)? {
        return client
            .icon_page(after, limit)
            .await
            .map_err(|e| e.to_string());
    }
    let pool = wait_for_sqlite_pool(app).await?;
    read::load_page(&pool, after, limit).await
}
pub async fn load_icon_for_exe<R: Runtime>(
    app: &AppHandle<R>,
    exe_name: &str,
) -> Result<Option<String>, String> {
    let result = if let Some(client) = crate::app::daemon_client::command_client(app)? {
        client
            .cached_icon(exe_name)
            .await
            .map_err(|e| e.to_string())?
    } else {
        let pool = wait_for_sqlite_pool(app).await?;
        read::lookup(&pool, exe_name).await?
    };
    Ok(result.icon.map(|icon| icon.data_url))
}
