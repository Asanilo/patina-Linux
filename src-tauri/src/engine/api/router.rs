use crate::engine::api::{auth, handlers, types::ApiError, types::RouteResponse};
use futures_util::FutureExt;
use serde::Serialize;
use std::panic::AssertUnwindSafe;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

pub async fn handle_connection(mut stream: TcpStream, app: tauri::AppHandle) {
    let (reader, mut writer) = stream.split();
    let mut buf_reader = BufReader::new(reader);
    let mut request_line = String::new();

    if let Err(error) = buf_reader.read_line(&mut request_line).await {
        eprintln!("[api] failed to read request line: {error}");
        return;
    }

    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        write_error_response(
            &mut writer,
            400,
            &ApiError::bad_request("malformed request"),
        )
        .await;
        return;
    }

    let method = parts[0];
    let path_with_query = parts[1];

    // Read headers to find Authorization and Content-Length
    let mut headers = Vec::new();
    let mut content_length: usize = 0;
    let mut authorization: Option<String> = None;

    loop {
        let mut header_line = String::new();
        match buf_reader.read_line(&mut header_line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let trimmed = header_line.trim();
                if trimmed.is_empty() {
                    break;
                }
                if let Some(value) = trimmed.strip_prefix("Content-Length:") {
                    content_length = value.trim().parse().unwrap_or(0);
                }
                if let Some(value) = trimmed.strip_prefix("Authorization:") {
                    authorization = Some(value.trim().to_string());
                }
                headers.push(header_line);
            }
        }
    }

    // Read body if present
    let mut body = vec![0u8; content_length.min(65536)];
    if content_length > 0 && content_length <= 65536 {
        let _ = tokio::io::AsyncReadExt::read_exact(&mut buf_reader, &mut body).await;
    }

    // Validate auth token
    if !auth::validate_token(authorization.as_deref()) {
        write_json_response(&mut writer, 401, &ApiError::unauthorized()).await;
        return;
    }

    // Split path and query
    let (path, query) = match path_with_query.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (path_with_query, None),
    };

    // Handle CORS preflight
    if method == "OPTIONS" {
        let response = format!(
            "HTTP/1.1 204 No Content\r\n\
             Access-Control-Allow-Origin: *\r\n\
             Access-Control-Allow-Headers: Authorization, Content-Type\r\n\
             Access-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\n\
             Access-Control-Max-Age: 86400\r\n\
             \r\n"
        );
        let _ = writer.write_all(response.as_bytes()).await;
        return;
    }

    let response = match AssertUnwindSafe(route_request(method, path, query, &body, &app))
        .catch_unwind()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            eprintln!("[api] handler panicked while serving {method} {path}");
            RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal("handler panicked"))
                    .unwrap_or_default(),
            }
        }
    };
    write_json_response(&mut writer, response.status, &response.body).await;
}

async fn route_request(
    method: &str,
    path: &str,
    query: Option<&str>,
    body: &[u8],
    app: &tauri::AppHandle,
) -> RouteResponse {
    match (method, path) {
        ("GET", "/api/v1/health") => handlers::health::get_health(app),
        ("GET", "/api/v1/diagnostics") => handlers::diagnostics::get_diagnostics(app).await,
        ("GET", "/api/v1/current") => handlers::health::get_current(app),
        ("GET", "/api/v1/sessions") => handlers::sessions::get_sessions(app, query).await,
        ("GET", "/api/v1/sessions/active") => handlers::sessions::get_active_session(app).await,
        ("GET", "/api/v1/summary/today") => handlers::sessions::get_summary_today(app).await,
        ("GET", "/api/v1/summary/range") => handlers::sessions::get_summary_range(app, query).await,
        ("GET", "/api/v1/summary/week") => handlers::sessions::get_summary_week(app).await,
        ("GET", "/api/v1/trend") => handlers::trend::get_trend(app, query).await,
        ("GET", "/api/v1/web-activity") => {
            handlers::web_activity::get_web_activity(app, query).await
        }
        ("GET", "/api/v1/apps") => handlers::apps::get_apps(app).await,
        ("POST", path) if path.starts_with("/api/v1/apps/") => {
            handlers::apps::handle_app_action(app, path, body).await
        }
        ("GET", "/api/v1/settings/tracker") => handlers::settings::get_tracker_settings(app).await,
        ("POST", "/api/v1/settings/tracker/afk-threshold") => {
            handlers::settings::set_afk_threshold(app, body).await
        }
        _ => RouteResponse {
            status: 404,
            body: serde_json::to_value(ApiError::not_found("endpoint not found"))
                .unwrap_or_default(),
        },
    }
}

async fn write_json_response<T: Serialize>(
    writer: &mut (impl AsyncWriteExt + Unpin),
    status: u16,
    body: &T,
) {
    let body_json = serde_json::to_string(body).unwrap_or_else(|_| "{}".to_string());
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown",
    };

    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Headers: Authorization, Content-Type\r\n\
         Access-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\n\
         \r\n\
         {body_json}",
        body_json.len()
    );

    let _ = writer.write_all(response.as_bytes()).await;
}

async fn write_error_response(
    writer: &mut (impl AsyncWriteExt + Unpin),
    status: u16,
    error: &ApiError,
) {
    write_json_response(writer, status, error).await;
}
