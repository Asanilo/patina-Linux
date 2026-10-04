use axum::{http::StatusCode, routing::get, Json, Router};
use patina_client::{Client, ClientError};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn fixture() -> Value {
    json!({"history":{"from_ms":0,"to_ms":100,"sampled_at_ms":100,
        "configuration_revision":"a".repeat(64),"tracking_health":{"status":"unavailable","last_heartbeat_ms":null,"live_cutoff_ms":0,"stale_after_ms":8000},
        "records":[{"origin":"native","record_id":1,"app_key":"editor","app_name":"Editor","exe_name":"editor","category":"development","display_name_override":null,"window_title":"caption","start_ms":10,"end_ms":60,"continuity_start_ms":10,"is_open":false,"title_samples":[]}]},
        "hours":(0..24).map(|hour|json!({"hour":hour,"active_ms":if hour==0 {50}else{0},"categories":if hour==0 {vec![json!({"category":"development","active_ms":50})]}else{vec![]}})).collect::<Vec<_>>()})
}

#[tokio::test]
async fn validates_hour_quantities_against_records_without_clock_inference() {
    let valid = fixture();
    for (index,value) in [valid.clone(),
        {let mut v=valid.clone();v["hours"][0]["active_ms"]=json!(51);v},
        {let mut v=valid.clone();v["hours"][0]["categories"][0]["category"]=json!("office");v},
        {let mut v=valid.clone();v["hours"][0]["active_ms"]=json!(100);v["hours"][0]["categories"]=json!([{"category":"development","active_ms":50},{"category":"development","active_ms":50}]);v},
        {let mut v=valid.clone();v["padding"]=json!("x".repeat(8*1024*1024));v},
    ].into_iter().enumerate() {
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client=Client::new(listener.local_addr().unwrap().port(),"fixture").unwrap();
        let app=Router::new().route("/api/v1/activity/history-product",get(move || {let value=value.clone();async move {Json(json!({"data":value}))}}));
        let server=tokio::spawn(async move {axum::serve(listener,app).await.unwrap()});
        let result=client.history_product(0,100,"en-US").await;
        if index==0 { assert_eq!(result.unwrap().hours[0].active_ms,50); } else { assert!(result.is_err()); }
        server.abort();
    }
}

#[tokio::test]
async fn missing_product_endpoint_is_not_replaced_with_old_history() {
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = Client::new(listener.local_addr().unwrap().port(), "fixture").unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().fallback(move || {
                let count = count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    StatusCode::NOT_FOUND
                }
            }),
        )
        .await
        .unwrap()
    });
    assert!(matches!(
        client.history_product(0, 100, "en-US").await,
        Err(ClientError::Http { status: 404, .. })
    ));
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    server.abort();
}
