use crate::engine::api::{
    context::ApiRuntimeContext,
    types::{ApiError, RouteResponse},
};
use serde_json::json;

pub async fn get_tools_snapshot(context: &ApiRuntimeContext) -> RouteResponse {
    match crate::engine::tools::get_snapshot_from_pool(context.pool(), context.now_ms()).await {
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
