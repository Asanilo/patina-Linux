use crate::engine::api::{
    context::ApiRuntimeContext,
    types::{ApiError, ApiResponse, RouteResponse},
};
use patina_protocol::icons::{MAX_ICON_KEY_BYTES, MAX_ICON_PAGE_ENTRIES};
fn error(status: u16, message: &str) -> RouteResponse {
    RouteResponse {
        status,
        body: serde_json::to_value(if status == 400 {
            ApiError::bad_request(message)
        } else {
            ApiError::internal(message)
        })
        .unwrap_or_default(),
    }
}
pub async fn get_icons(
    context: &ApiRuntimeContext,
    query: Option<&str>,
    single: bool,
) -> RouteResponse {
    let mut key = None;
    let mut after = None;
    let mut limit = None;
    for (name, value) in url::form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        match name.as_ref() {
            "key"
                if single
                    && key.is_none()
                    && !value.is_empty()
                    && value.len() <= MAX_ICON_KEY_BYTES =>
            {
                key = Some(value.into_owned())
            }
            "after" if !single && after.is_none() && value.len() <= MAX_ICON_KEY_BYTES => {
                after = Some(value.into_owned())
            }
            "limit" if !single && limit.is_none() => {
                let Ok(value) = value.parse::<usize>() else {
                    return error(400, "invalid icon limit");
                };
                if !(1..=MAX_ICON_PAGE_ENTRIES).contains(&value) {
                    return error(400, "invalid icon limit");
                }
                limit = Some(value);
            }
            _ => return error(400, "unsupported or duplicate icon query parameter"),
        }
    }
    let result = if single {
        let Some(key) = key else {
            return error(400, "icon key is required");
        };
        crate::data::repositories::icon_cache::read::lookup(context.pool(), &key)
            .await
            .and_then(|data| serde_json::to_value(ApiResponse { data }).map_err(|e| e.to_string()))
    } else {
        crate::data::repositories::icon_cache::read::load_page(
            context.pool(),
            after.as_deref(),
            limit.unwrap_or(MAX_ICON_PAGE_ENTRIES),
        )
        .await
        .and_then(|data| serde_json::to_value(ApiResponse { data }).map_err(|e| e.to_string()))
    };
    match result {
        Ok(body) => RouteResponse { status: 200, body },
        Err(message) => error(500, &message),
    }
}
