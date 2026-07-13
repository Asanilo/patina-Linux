use crate::engine::api::{auth::ApiCredentialStore, types::ApiError, types::RouteResponse};
use std::future::Future;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

const REQUEST_LINE_LIMIT: usize = 8 * 1024;
const HEADER_SECTION_LIMIT: usize = 32 * 1024;
const BODY_LIMIT: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiRequest {
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub body: Vec<u8>,
    pub authorization: Option<String>,
}

pub async fn serve_connection<S, F, Fut>(stream: S, credentials: ApiCredentialStore, route: F)
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: FnOnce(ApiRequest) -> Fut,
    Fut: Future<Output = RouteResponse>,
{
    let (reader, mut writer) = tokio::io::split(stream);
    let request = match tokio::time::timeout(REQUEST_TIMEOUT, parse_request(reader)).await {
        Ok(Ok(request)) => request,
        Ok(Err(error)) => {
            write_json_response(&mut writer, error.status, &error.api_error()).await;
            return;
        }
        Err(_) => return,
    };

    if request.method == "OPTIONS" {
        write_preflight_response(&mut writer).await;
        return;
    }
    if !credentials.validate(request.authorization.as_deref()) {
        write_json_response(&mut writer, 401, &ApiError::unauthorized()).await;
        return;
    }

    let response = route(request).await;
    write_json_response(&mut writer, response.status, &response.body).await;
}

async fn parse_request<R: AsyncRead + Unpin>(reader: R) -> Result<ApiRequest, ParseError> {
    let mut reader = BufReader::new(reader);
    let request_line = read_bounded_line(&mut reader, REQUEST_LINE_LIMIT).await?;
    let parts = request_line.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 3 || !parts[2].starts_with("HTTP/") {
        return Err(ParseError::bad_request("malformed request"));
    }

    let mut header_bytes = 0;
    let mut content_length = 0_usize;
    let mut authorization = None;
    loop {
        let remaining = HEADER_SECTION_LIMIT.saturating_sub(header_bytes);
        if remaining == 0 {
            return Err(ParseError::payload_too_large("header section is too large"));
        }
        let line = read_bounded_line(&mut reader, remaining).await?;
        header_bytes += line.len() + 2;
        if line.is_empty() {
            break;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| ParseError::bad_request("malformed header"))?;
        match name.trim().to_ascii_lowercase().as_str() {
            "authorization" => authorization = Some(value.trim().to_string()),
            "content-length" => {
                content_length = value
                    .trim()
                    .parse()
                    .map_err(|_| ParseError::bad_request("invalid content-length"))?;
            }
            _ => {}
        }
    }
    if content_length > BODY_LIMIT {
        return Err(ParseError::payload_too_large("request body is too large"));
    }

    let mut body = vec![0_u8; content_length];
    if content_length > 0 {
        reader
            .read_exact(&mut body)
            .await
            .map_err(|_| ParseError::bad_request("incomplete request body"))?;
    }
    let (path, query) = parts[1]
        .split_once('?')
        .map(|(path, query)| (path.to_string(), Some(query.to_string())))
        .unwrap_or_else(|| (parts[1].to_string(), None));

    Ok(ApiRequest {
        method: parts[0].to_string(),
        path,
        query,
        body,
        authorization,
    })
}

async fn read_bounded_line<R: AsyncRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> Result<String, ParseError> {
    let mut bytes = Vec::with_capacity(limit.min(256));
    loop {
        let byte = reader
            .read_u8()
            .await
            .map_err(|_| ParseError::bad_request("incomplete request headers"))?;
        if byte == b'\n' {
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            return String::from_utf8(bytes)
                .map_err(|_| ParseError::bad_request("request headers are not UTF-8"));
        }
        if bytes.len() >= limit {
            return Err(ParseError::payload_too_large(
                "request headers are too large",
            ));
        }
        bytes.push(byte);
    }
}

struct ParseError {
    status: u16,
    message: &'static str,
}

impl ParseError {
    fn bad_request(message: &'static str) -> Self {
        Self {
            status: 400,
            message,
        }
    }

    fn payload_too_large(message: &'static str) -> Self {
        Self {
            status: 413,
            message,
        }
    }

    fn api_error(&self) -> ApiError {
        ApiError::bad_request(self.message)
    }
}

async fn write_preflight_response<W: AsyncWrite + Unpin>(writer: &mut W) {
    let response = "HTTP/1.1 204 No Content\r\n\
                    Content-Length: 0\r\n\
                    Connection: close\r\n\
                    Access-Control-Allow-Origin: *\r\n\
                    Access-Control-Allow-Headers: Authorization, Content-Type\r\n\
                    Access-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\n\
                    Access-Control-Max-Age: 86400\r\n\r\n";
    let _ = writer.write_all(response.as_bytes()).await;
}

async fn write_json_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    status: u16,
    body: &impl serde::Serialize,
) {
    let body = serde_json::to_string(body).unwrap_or_else(|_| "{}".to_string());
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Unknown",
    };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Authorization, Content-Type\r\nAccess-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\n\r\n{body}",
        body.len()
    );
    let _ = writer.write_all(response.as_bytes()).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    fn credentials() -> ApiCredentialStore {
        let path = std::env::temp_dir().join(format!(
            "patina-http-token-{}-{}",
            std::process::id(),
            crate::app::runtime::now_ms()
        ));
        let store = ApiCredentialStore::new();
        store.initialize_at(&path, Some("test-token")).unwrap();
        store
    }

    async fn exchange(request: &str) -> String {
        let (mut client, server) = tokio::io::duplex(256 * 1024);
        let task = tokio::spawn(serve_connection(
            server,
            credentials(),
            |request| async move {
                RouteResponse {
                    status: 200,
                    body: json!({
                        "method": request.method,
                        "path": request.path,
                        "query": request.query,
                        "body": String::from_utf8_lossy(&request.body),
                    }),
                }
            },
        ));
        client.write_all(request.as_bytes()).await.unwrap();
        client.shutdown().await.unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).await.unwrap();
        task.await.unwrap();
        response
    }

    #[tokio::test]
    async fn lowercase_headers_and_valid_bearer_reach_route() {
        let response = exchange(
            "POST /items?limit=2 HTTP/1.1\r\nauthorization: Bearer test-token\r\ncontent-length: 4\r\n\r\ntest",
        )
        .await;

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("\"path\":\"/items\""));
        assert!(response.contains("\"query\":\"limit=2\""));
        assert!(response.contains("\"body\":\"test\""));
    }

    #[tokio::test]
    async fn malformed_and_oversized_requests_are_rejected() {
        let malformed = exchange("GET /missing-version\r\n\r\n").await;
        assert!(malformed.starts_with("HTTP/1.1 400 Bad Request"));

        let oversized_body = exchange(&format!(
            "POST /items HTTP/1.1\r\nAuthorization: Bearer test-token\r\nContent-Length: {}\r\n\r\n",
            BODY_LIMIT + 1
        ))
        .await;
        assert!(oversized_body.starts_with("HTTP/1.1 413 Payload Too Large"));

        let oversized_line = exchange(&format!(
            "GET /{} HTTP/1.1\r\n\r\n",
            "x".repeat(REQUEST_LINE_LIMIT)
        ))
        .await;
        assert!(oversized_line.starts_with("HTTP/1.1 413 Payload Too Large"));

        let oversized_headers = exchange(&format!(
            "GET /items HTTP/1.1\r\nX-Oversized: {}\r\n\r\n",
            "x".repeat(HEADER_SECTION_LIMIT)
        ))
        .await;
        assert!(oversized_headers.starts_with("HTTP/1.1 413 Payload Too Large"));
    }

    #[tokio::test]
    async fn missing_token_is_rejected() {
        let response = exchange("GET /health HTTP/1.1\r\n\r\n").await;
        assert!(response.starts_with("HTTP/1.1 401 Unauthorized"));
    }

    #[tokio::test]
    async fn options_bypasses_auth_and_route_handler() {
        let (mut client, server) = tokio::io::duplex(4096);
        let invoked = Arc::new(AtomicBool::new(false));
        let invoked_by_route = invoked.clone();
        let task = tokio::spawn(serve_connection(
            server,
            credentials(),
            move |_| async move {
                invoked_by_route.store(true, Ordering::SeqCst);
                RouteResponse {
                    status: 500,
                    body: json!({}),
                }
            },
        ));
        client
            .write_all(b"OPTIONS /api/v1/health HTTP/1.1\r\n\r\n")
            .await
            .unwrap();
        client.shutdown().await.unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).await.unwrap();
        task.await.unwrap();

        assert!(response.starts_with("HTTP/1.1 204 No Content"));
        assert!(response.contains("Content-Length: 0"));
        assert!(response.contains("Access-Control-Allow-Origin: *"));
        assert!(!invoked.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn stalled_headers_time_out_and_close() {
        let (mut client, server) = tokio::io::duplex(4096);
        let task = tokio::spawn(serve_connection(server, credentials(), |_| async {
            RouteResponse {
                status: 200,
                body: json!({}),
            }
        }));
        client
            .write_all(b"GET /health HTTP/1.1\r\nAuthorization:")
            .await
            .unwrap();

        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .expect("stalled connection should time out")
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        assert!(response.is_empty());
    }
}
