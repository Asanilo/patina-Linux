use crate::engine::api::{
    context::ApiRuntimeContext,
    types::{ApiError, ApiResponse, RouteResponse},
};

pub async fn get_dashboard(context: &ApiRuntimeContext, query: Option<&str>) -> RouteResponse {
    let (date, language) = match parameters(query) {
        Ok(value) => value,
        Err(error) => return error_response(400, &error),
    };
    let boundaries = match crate::domain::activity_calendar::dashboard_boundaries(&date) {
        Ok(value) => value,
        Err(error) => return error_response(400, &error),
    };
    match crate::data::repositories::daily_activity::load_dashboard_product(
        context.pool(),
        &boundaries,
        context.now_ms(),
        &language,
    )
    .await
    {
        Ok(data) => RouteResponse {
            status: 200,
            body: serde_json::to_value(ApiResponse { data }).unwrap_or_default(),
        },
        Err(error) => error_response(500, &error),
    }
}

fn parameters(query: Option<&str>) -> Result<(String, String), String> {
    let mut date = None;
    let mut language = None;
    for (key, value) in url::form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        match key.as_ref() {
            "date" if date.is_none() => date = Some(value.into_owned()),
            "language" if language.is_none() && matches!(value.as_ref(), "en-US" | "zh-CN") => {
                language = Some(value.into_owned())
            }
            _ => return Err("invalid or duplicate Dashboard query parameter".into()),
        }
    }
    Ok((
        date.ok_or("Dashboard date is required")?,
        language.unwrap_or_else(|| "en-US".into()),
    ))
}

fn error_response(status: u16, message: &str) -> RouteResponse {
    let error = if status == 400 {
        ApiError::bad_request(message)
    } else {
        ApiError::internal(message)
    };
    RouteResponse {
        status,
        body: serde_json::to_value(error).unwrap_or_default(),
    }
}
