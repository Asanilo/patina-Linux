use crate::engine::api::types::{ApiError, RouteResponse};
use serde_json::json;

pub async fn get_tools_snapshot(app: &tauri::AppHandle) -> RouteResponse {
    match crate::engine::tools::get_snapshot(app).await {
        Ok(snapshot) => RouteResponse {
            status: 200,
            body: json!({ "data": snapshot }),
        },
        Err(error) => RouteResponse {
            status: 500,
            body: serde_json::to_value(ApiError::internal(&error)).unwrap_or_default(),
        },
    }
}
