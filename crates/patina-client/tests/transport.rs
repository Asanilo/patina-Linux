use patina_client::{protocol::CapabilitiesResponse, Client, ClientError, MAX_RESPONSE_BYTES};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

async fn fixture(response: String) -> (Client, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = Client::new(listener.local_addr().unwrap().port(), "fixture-secret").unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let mut chunk = [0u8; 1024];
            let n = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut chunk))
                .await
                .unwrap()
                .unwrap();
            assert!(n > 0);
            request.extend_from_slice(&chunk[..n]);
            if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                let length = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map(|s| s.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if request.len() >= end + 4 + length {
                    break;
                }
            }
            assert!(request.len() < 64 * 1024);
        }
        // Oversize/redirect tests may deliberately close before consuming the body.
        let _ = socket.write_all(response.as_bytes()).await;
        String::from_utf8(request).unwrap()
    });
    (client, server)
}

fn response(status: &str, body: &str, headers: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
        body.len()
    )
}

fn capabilities() -> Value {
    json!({"server_version":"1.9.2","protocol_version":2,
        "protocol":{"current":2,"min_supported_client":1,"max_supported_client":2},
        "runtime_host":"daemon","event_stream":{"available":true},
        "tracking":{"owned":true,"ready":true},"tools":{"owned":true,"ready":true},
        "browser_activity_bridge":{"owned":true,"ready":true},
        "daemon_service":{"owned":true,"ready":true},"write_api":{"available":true,"operations":["classification"]}})
}

#[tokio::test]
async fn independent_client_negotiates_the_existing_protocol_and_sends_only_header_auth() {
    let (client, server) = fixture(response(
        "200 OK",
        &json!({"data":capabilities()}).to_string(),
        "Content-Type: application/json\r\n",
    ))
    .await;
    let accepted = client.negotiate_tracking_owner().await.unwrap();
    assert_eq!(accepted.server_version, "1.9.2");
    let request = server.await.unwrap();
    assert!(request.starts_with("GET /api/v1/capabilities HTTP/1.1\r\n"));
    assert!(request
        .to_ascii_lowercase()
        .contains("authorization: bearer fixture-secret\r\n"));
    assert!(!format!("{client:?}").contains("fixture-secret"));
}

#[tokio::test]
async fn negotiation_rejects_wrong_owner_missing_events_and_inconsistent_versions() {
    for (key, value, code) in [
        ("runtime_host", json!("desktop"), "wrong-runtime-host"),
        (
            "tracking",
            json!({"owned":false,"ready":false}),
            "tracking-not-owned",
        ),
        (
            "event_stream",
            json!({"available":false}),
            "event-stream-unavailable",
        ),
        ("protocol_version", json!(3), "incompatible-protocol"),
    ] {
        let mut input = capabilities();
        input[key] = value;
        let dto: CapabilitiesResponse = serde_json::from_value(input).unwrap();
        assert_eq!(
            patina_client::negotiate_tracking_capabilities(dto)
                .unwrap_err()
                .code(),
            code
        );
    }
    let mut newer = capabilities();
    newer["protocol_version"] = json!(3);
    newer["protocol"]["current"] = json!(3);
    newer["protocol"]["max_supported_client"] = json!(3);
    assert!(
        patina_client::negotiate_tracking_capabilities(serde_json::from_value(newer).unwrap())
            .is_ok()
    );
}

#[tokio::test]
async fn redirects_never_forward_credentials_to_another_listener() {
    let other = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let headers = format!(
        "Location: http://127.0.0.1:{}/capture\r\n",
        other.local_addr().unwrap().port()
    );
    let (client, server) = fixture(response("302 Found", "", &headers)).await;
    assert!(matches!(
        client.capabilities().await,
        Err(ClientError::Http { status: 302, .. })
    ));
    server.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), other.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn errors_and_body_budgets_remain_distinct_from_empty_success() {
    let cases = [
        (response("401 Unauthorized", "{}", ""), "unauthorized"),
        (response("503 Service Unavailable", r#"{"error":{"code":"busy","message":"try later"}}"#, ""), "http-error"),
        (response("200 OK", "invalid-json", ""), "invalid-response"),
        (response("200 OK", &"x".repeat(MAX_RESPONSE_BYTES + 1), ""), "response-too-large"),
        (format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n", MAX_RESPONSE_BYTES + 1, "x".repeat(MAX_RESPONSE_BYTES + 1)), "response-too-large"),
    ];
    for (wire, code) in cases {
        let (client, server) = fixture(wire).await;
        assert_eq!(
            client
                .get_json::<Value>("/api/v1/test", "test")
                .await
                .unwrap_err()
                .code(),
            code
        );
        server.await.unwrap();
    }
}

#[tokio::test]
async fn post_is_sent_once_and_not_retried_on_ambiguous_failure() {
    let (client, server) = fixture(response("503 Service Unavailable", "{}", "")).await;
    assert!(matches!(
        client
            .post_ack(
                "/api/v1/settings/classification",
                &json!({"fixture":true}),
                "classification"
            )
            .await,
        Err(ClientError::Http { status: 503, .. })
    ));
    let request = server.await.unwrap();
    assert!(request.starts_with("POST /api/v1/settings/classification HTTP/1.1"));
    assert!(request.ends_with(r#"{"fixture":true}"#));
}

#[tokio::test]
async fn api_paths_cannot_escape_the_fixed_loopback_boundary() {
    let client = Client::new(1, "fixture-secret").unwrap();
    for path in [
        "https://example.test/",
        "//example.test/",
        "/api/v1/../../secret",
        "/api/v1/%2e%2e/%2e%2e/secret",
        "/api/v1/test#fragment",
        "/api/v1/\\secret",
        "/api/v1/test\r\nheader: value",
    ] {
        assert_eq!(
            client
                .get_json::<Value>(path, "test")
                .await
                .unwrap_err()
                .code(),
            "invalid-configuration"
        );
    }
}

#[tokio::test]
async fn event_stream_preserves_cursor_and_rejects_wrong_content_type() {
    let body = "id: 8\nevent: tracking-data-changed\ndata: {\"sequence\":8}\n\n";
    let (client, server) = fixture(response(
        "200 OK",
        body,
        "Content-Type: text/event-stream; charset=utf-8\r\n",
    ))
    .await;
    let mut events = client.open_event_stream(Some(7)).await.unwrap();
    let event = events.next_event().await.unwrap().unwrap();
    assert_eq!(event.id, "8");
    assert_eq!(event.event, "tracking-data-changed");
    assert!(events.next_event().await.unwrap().is_none());
    assert!(server
        .await
        .unwrap()
        .to_ascii_lowercase()
        .contains("last-event-id: 7\r\n"));
    let (client, server) = fixture(response(
        "200 OK",
        "{}",
        "Content-Type: application/json\r\n",
    ))
    .await;
    assert!(matches!(
        client.open_event_stream(None).await,
        Err(ClientError::InvalidResponse(_))
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn incomplete_sse_event_is_rejected_before_unbounded_buffering() {
    let (client, server) = fixture(response(
        "200 OK",
        &format!("data: {}", "x".repeat(256 * 1024)),
        "Content-Type: text/event-stream\r\n",
    ))
    .await;
    let mut events = client.open_event_stream(None).await.unwrap();
    assert!(events.next_event().await.is_err());
    server.await.unwrap();
}
