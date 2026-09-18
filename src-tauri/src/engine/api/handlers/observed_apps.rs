use crate::engine::api::{
    context::ApiRuntimeContext,
    types::{ApiError, ApiResponse, RouteResponse},
};

pub async fn get_observed_apps(context: &ApiRuntimeContext, query: Option<&str>) -> RouteResponse {
    let (from, to) = match parse_range(query) {
        Ok(range) => range,
        Err(message) => {
            return RouteResponse {
                status: 400,
                body: serde_json::to_value(ApiError::bad_request(&message)).unwrap_or_default(),
            }
        }
    };
    match crate::data::repositories::observed_apps::load_observed_apps(
        context.pool(),
        from,
        to,
        context.now_ms(),
    )
    .await
    {
        Ok(data) => RouteResponse {
            status: 200,
            body: serde_json::to_value(ApiResponse { data }).unwrap_or_default(),
        },
        Err(message) => RouteResponse {
            status: 500,
            body: serde_json::to_value(ApiError::internal(&message)).unwrap_or_default(),
        },
    }
}

fn parse_range(query: Option<&str>) -> Result<(i64, i64), String> {
    let mut from = None;
    let mut to = None;
    for (key, value) in url::form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        let slot = match key.as_ref() {
            "from_ms" => &mut from,
            "to_ms" => &mut to,
            _ => return Err("unsupported observed apps parameter".into()),
        };
        if slot.is_some() {
            return Err("duplicate observed apps parameter".into());
        }
        *slot = Some(
            value
                .parse::<i64>()
                .map_err(|_| "invalid observed apps timestamp")?,
        );
    }
    let range = (
        from.ok_or("from_ms is required")?,
        to.ok_or("to_ms is required")?,
    );
    crate::domain::observed_apps::validate_range(range.0, range.1)?;
    Ok(range)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_unambiguous_bounded_range() {
        for query in [
            "",
            "from_ms=0",
            "from_ms=1&to_ms=1",
            "from_ms=-1&to_ms=2",
            "from_ms=0&to_ms=9999999999999",
            "from_ms=0&to_ms=2&from_ms=1",
            "from_ms=0&to_ms=2&limit=1",
            "from_ms=x&to_ms=2",
        ] {
            assert!(parse_range(Some(query)).is_err(), "{query}");
        }
        assert_eq!(parse_range(Some("from_ms=0&to_ms=2")).unwrap(), (0, 2));
    }
}
