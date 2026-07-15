use crate::engine::api::{handlers, types::RouteResponse};
use serde_json::json;

pub async fn get_activity_context(
    context: &crate::engine::api::context::ApiRuntimeContext,
) -> RouteResponse {
    let diagnostics = handlers::diagnostics::get_diagnostics(context).await;
    let active_session = handlers::sessions::get_active_session(context).await;
    let today_summary = handlers::sessions::get_summary_today(context).await;
    let week_summary = handlers::sessions::get_summary_week(context).await;
    let recent_web_activity =
        handlers::web_activity::get_web_activity(context, Some("limit=25")).await;

    RouteResponse {
        status: 200,
        body: json!({
            "data": {
                "diagnostics": response_data(diagnostics),
                "active_session": response_data(active_session),
                "today_summary": response_data(today_summary),
                "week_summary": response_data(week_summary),
                "recent_web_activity": response_data(recent_web_activity)
            }
        }),
    }
}

fn response_data(response: RouteResponse) -> serde_json::Value {
    if response.status == 200 {
        response
            .body
            .get("data")
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    } else {
        json!({
            "error": response.body
        })
    }
}
