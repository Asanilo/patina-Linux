use crate::engine::api::{
    context::ApiRuntimeContext,
    types::{ApiError, ApiResponse, RouteResponse},
};

pub async fn get_history(context: &ApiRuntimeContext, query: Option<&str>) -> RouteResponse {
    let (from, to, language) = match parameters(query) {
        Ok(value) => value,
        Err(message) => return error_response(400, &message),
    };
    match crate::data::repositories::exact_history::load_exact_history(
        context.pool(),
        from,
        to,
        context.now_ms(),
        &language,
    )
    .await
    {
        Ok(data) => RouteResponse {
            status: 200,
            body: serde_json::to_value(ApiResponse { data }).unwrap_or_default(),
        },
        Err(message) => error_response(500, &message),
    }
}

fn parameters(query: Option<&str>) -> Result<(i64, i64, String), String> {
    let mut from = None;
    let mut to = None;
    let mut language = None;
    for (key, value) in url::form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        match key.as_ref() {
            "from_ms" if from.is_none() => {
                from = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| "invalid exact history start")?,
                )
            }
            "to_ms" if to.is_none() => {
                to = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| "invalid exact history end")?,
                )
            }
            "language" if language.is_none() && matches!(value.as_ref(), "en-US" | "zh-CN") => {
                language = Some(value.into_owned())
            }
            _ => return Err("unsupported or duplicate exact history parameter".into()),
        }
    }
    let from = from.ok_or("exact history start is required")?;
    let to = to.ok_or("exact history end is required")?;
    if !patina_protocol::history::valid_range(from, to) {
        return Err("exact history requires a positive range of at most 32 days".into());
    }
    Ok((from, to, language.unwrap_or_else(|| "en-US".into())))
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
