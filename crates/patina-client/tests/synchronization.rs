use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue},
    response::{sse::Event, IntoResponse, Sse},
    routing::get,
    Json, Router,
};
use futures_util::{future::BoxFuture, stream};
use patina_client::protocol::events::{
    RuntimeEvent, RuntimeEventEnvelope, ToolAlert, ToolAlertKind,
};
use patina_client::{
    state::ClientState,
    sync::{ConnectionStatus, SessionOptions, SnapshotOutput, SnapshotReader, SnapshotSession},
    Client, ClientError,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::VecDeque,
    convert::Infallible,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{broadcast, watch, Notify};

#[derive(Default)]
struct Gate {
    blocked: AtomicBool,
    count: AtomicU64,
    release: Notify,
}
impl Gate {
    fn block(&self) {
        self.blocked.store(true, Ordering::SeqCst);
    }
    fn release(&self) {
        self.blocked.store(false, Ordering::SeqCst);
        self.release.notify_waiters();
    }
    async fn enter(&self) {
        self.count.fetch_add(1, Ordering::SeqCst);
        loop {
            let released = self.release.notified();
            if !self.blocked.load(Ordering::SeqCst) {
                break;
            }
            released.await;
        }
    }
}

#[derive(Clone)]
enum Wire {
    Event(RuntimeEventEnvelope),
    Resync,
    Close,
}
struct ServerState {
    value: AtomicU64,
    sequence: AtomicU64,
    epoch: Mutex<Option<String>>,
    history: Mutex<Vec<RuntimeEventEnvelope>>,
    requested_cursors: Mutex<Vec<Option<u64>>>,
    events: broadcast::Sender<Wire>,
    capability_gate: Gate,
    stream_gate: Gate,
    snapshot_gate: Gate,
}
struct Fixture {
    client: Client,
    state: Arc<ServerState>,
    task: tokio::task::JoinHandle<()>,
}
impl Fixture {
    async fn start(value: u64, epoch: Option<&str>) -> Self {
        let state = Arc::new(ServerState {
            value: AtomicU64::new(value),
            sequence: AtomicU64::new(0),
            epoch: Mutex::new(epoch.map(str::to_owned)),
            history: Mutex::new(Vec::new()),
            requested_cursors: Mutex::new(Vec::new()),
            events: broadcast::channel(32).0,
            capability_gate: Gate::default(),
            stream_gate: Gate::default(),
            snapshot_gate: Gate::default(),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = Client::new(listener.local_addr().unwrap().port(), "sync-fixture").unwrap();
        let app = Router::new()
            .route("/api/v1/capabilities", get(capabilities))
            .route("/api/v1/events", get(events))
            .route("/api/v1/fixture", get(snapshot))
            .with_state(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            client,
            state,
            task,
        }
    }
    fn emit(&self, event: RuntimeEvent) -> u64 {
        let sequence = self.state.sequence.fetch_add(1, Ordering::SeqCst) + 1;
        let event = RuntimeEventEnvelope { sequence, event };
        self.state.history.lock().unwrap().push(event.clone());
        let _ = self.state.events.send(Wire::Event(event));
        sequence
    }
    fn change(&self, value: u64) -> u64 {
        self.state.value.store(value, Ordering::SeqCst);
        self.emit(RuntimeEvent::TrackingDataChanged {
            reason: "fixture".into(),
            changed_at_ms: value,
        })
    }
    fn alert(&self) -> u64 {
        self.emit(RuntimeEvent::ToolAlert {
            alert: ToolAlert {
                id: "fixture".into(),
                kind: ToolAlertKind::Reminder,
                title: "fixture".into(),
                body: "fixture".into(),
                occurred_at: 1,
            },
        })
    }
    fn close(&self) {
        let _ = self.state.events.send(Wire::Close);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.state.capability_gate.release();
        self.state.stream_gate.release();
        self.state.snapshot_gate.release();
        self.task.abort();
    }
}
fn authenticate(headers: &HeaderMap) {
    assert_eq!(headers.get("authorization").unwrap(), "Bearer sync-fixture");
}
async fn capabilities(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    authenticate(&headers);
    state.capability_gate.enter().await;
    Json(
        json!({"data":{"server_version":"fixture","protocol_version":2,"protocol":{"current":2,"min_supported_client":1,"max_supported_client":2},
        "runtime_host":"daemon","event_stream":{"available":true},"tracking":{"owned":true,"ready":true},
        "browser_activity_bridge":{"owned":true,"ready":false},"tools":{"owned":true,"ready":true},
        "daemon_service":{"owned":false,"ready":false},"write_api":{"available":false,"operations":[]}}}),
    )
}
async fn snapshot(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    authenticate(&headers);
    let value = state.value.load(Ordering::SeqCst);
    state.snapshot_gate.enter().await;
    Json(json!({"data":{"value":value}}))
}
async fn events(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
) -> axum::response::Response {
    authenticate(&headers);
    state.stream_gate.enter().await;
    let receiver = state.events.subscribe();
    let cursor = headers
        .get("last-event-id")
        .map(|s| s.to_str().unwrap().parse::<u64>().unwrap());
    state.requested_cursors.lock().unwrap().push(cursor);
    let replay: VecDeque<_> = state
        .history
        .lock()
        .unwrap()
        .iter()
        .filter(|event| cursor.is_some_and(|c| event.sequence > c))
        .cloned()
        .map(Wire::Event)
        .collect();
    let output = stream::unfold(
        (receiver, replay),
        |(mut receiver, mut replay)| async move {
            let item = match replay.pop_front() {
                Some(item) => item,
                None => receiver.recv().await.ok()?,
            };
            let event = match item {
                Wire::Close => return None,
                Wire::Resync => Event::default()
                    .event("resync-required")
                    .data(r#"{"reason":"replay-gap","missed":3}"#),
                Wire::Event(event) => Event::default()
                    .id(event.sequence.to_string())
                    .event(event.event.event_name())
                    .data(serde_json::to_string(&event).unwrap()),
            };
            Some((Ok::<_, Infallible>(event), (receiver, replay)))
        },
    );
    let mut response = Sse::new(output).into_response();
    if let Some(epoch) = state.epoch.lock().unwrap().as_ref() {
        response.headers_mut().insert(
            patina_client::protocol::EVENT_INSTANCE_HEADER,
            HeaderValue::from_str(epoch).unwrap(),
        );
    }
    response
}

#[derive(Clone, Debug, Deserialize)]
struct Snapshot {
    value: u64,
    #[serde(default)]
    cursor: Option<u64>,
}
struct Reader;
impl SnapshotReader for Reader {
    type Snapshot = Snapshot;
    fn read<'a>(
        &'a self,
        client: &'a Client,
        cursor: Option<u64>,
    ) -> BoxFuture<'a, Result<Snapshot, ClientError>> {
        Box::pin(async move {
            let mut snapshot: Snapshot = client.get_json("/api/v1/fixture", "fixture").await?;
            snapshot.cursor = cursor;
            Ok(snapshot)
        })
    }
    fn needs_refresh(&self, _: &Snapshot, event: &RuntimeEventEnvelope) -> bool {
        matches!(event.event, RuntimeEvent::TrackingDataChanged { .. })
    }
}
#[derive(Default)]
struct Output {
    snapshots: Mutex<Vec<Snapshot>>,
    events: Mutex<Vec<RuntimeEventEnvelope>>,
    resyncs: Mutex<Vec<String>>,
    statuses: Mutex<Vec<ConnectionStatus>>,
}
impl SnapshotOutput<Snapshot> for Output {
    fn connection_changed(&self, status: ConnectionStatus, _: Option<&ClientError>) {
        self.statuses.lock().unwrap().push(status);
    }
    fn snapshot_changed(&self, snapshot: Snapshot) {
        self.snapshots.lock().unwrap().push(snapshot);
    }
    fn tracking_data_changed(&self, event: &RuntimeEventEnvelope) {
        self.events.lock().unwrap().push(event.clone());
    }
    fn resync_required(&self, reason: &str, _: Option<u64>) {
        self.resyncs.lock().unwrap().push(reason.into());
    }
}
struct Running {
    state: ClientState,
    output: Arc<Output>,
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}
impl Running {
    fn start(client: Client) -> Self {
        let state = ClientState::default();
        state.install(client);
        let output = Arc::new(Output::default());
        let (stop, rx) = watch::channel(false);
        let session = SnapshotSession::new(state.clone(), Reader, output.clone())
            .with_options(SessionOptions {
                refresh_interval: Duration::from_secs(60),
                initial_retry_delay: Duration::from_millis(10),
                max_retry_delay: Duration::from_millis(40),
            })
            .unwrap();
        let task = tokio::spawn(async move {
            session.run(rx).await;
        });
        Self {
            state,
            output,
            stop,
            task,
        }
    }
    async fn stop(self) {
        self.stop.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(1), self.task)
            .await
            .expect("stop must cancel in-flight reads")
            .unwrap();
    }
}
async fn until(mut check: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("condition not reached");
}

#[tokio::test]
async fn shutdown_cancels_negotiation_subscription_and_initial_snapshot() {
    for stage in 0..3 {
        let fixture = Fixture::start(1, Some("first")).await;
        let gate = match stage {
            0 => &fixture.state.capability_gate,
            1 => &fixture.state.stream_gate,
            _ => &fixture.state.snapshot_gate,
        };
        gate.block();
        let running = Running::start(fixture.client.clone());
        until(|| gate.count.load(Ordering::SeqCst) > 0).await;
        let output = running.output.clone();
        running.stop().await;
        gate.release();
        assert!(output.snapshots.lock().unwrap().is_empty());
        assert!(!output
            .statuses
            .lock()
            .unwrap()
            .contains(&ConnectionStatus::Ready));
        assert_eq!(
            output.statuses.lock().unwrap().last(),
            Some(&ConnectionStatus::Stopped)
        );
    }
}

#[tokio::test]
async fn reconfiguration_discards_old_in_flight_snapshot_and_rebinds_immediately() {
    let old = Fixture::start(1, Some("old")).await;
    old.state.snapshot_gate.block();
    let new = Fixture::start(2, Some("new")).await;
    let running = Running::start(old.client.clone());
    until(|| old.state.snapshot_gate.count.load(Ordering::SeqCst) > 0).await;
    running.state.install(new.client.clone());
    until(|| {
        running
            .output
            .snapshots
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.value == 2)
    })
    .await;
    old.state.snapshot_gate.release();
    new.change(3);
    until(|| {
        running
            .output
            .snapshots
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.value == 3)
    })
    .await;
    assert!(!running
        .output
        .snapshots
        .lock()
        .unwrap()
        .iter()
        .any(|s| s.value == 1));
    running.stop().await;
}

#[tokio::test]
async fn subscribing_before_snapshot_catches_a_commit_during_the_initial_read() {
    let fixture = Fixture::start(1, Some("same")).await;
    fixture.state.snapshot_gate.block();
    let running = Running::start(fixture.client.clone());
    until(|| fixture.state.snapshot_gate.count.load(Ordering::SeqCst) == 1).await;
    assert_eq!(
        fixture.state.requested_cursors.lock().unwrap().as_slice(),
        &[None]
    );
    fixture.change(2);
    fixture.state.snapshot_gate.release();
    until(|| {
        running
            .output
            .snapshots
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.value == 2 && s.cursor == Some(1))
    })
    .await;
    assert_eq!(running.output.events.lock().unwrap().len(), 1);
    running.stop().await;
}

#[tokio::test]
async fn same_instance_replays_missed_events_without_repeating_alerts() {
    let fixture = Fixture::start(1, Some("same")).await;
    let running = Running::start(fixture.client.clone());
    until(|| {
        running
            .output
            .statuses
            .lock()
            .unwrap()
            .contains(&ConnectionStatus::Ready)
    })
    .await;
    fixture.alert();
    until(|| running.output.events.lock().unwrap().len() == 1).await;
    let duplicate = fixture.state.history.lock().unwrap()[0].clone();
    let _ = fixture.state.events.send(Wire::Event(duplicate));
    fixture.state.stream_gate.block();
    fixture.close();
    until(|| fixture.state.stream_gate.count.load(Ordering::SeqCst) >= 2).await;
    fixture.change(2);
    fixture.state.stream_gate.release();
    until(|| running.output.events.lock().unwrap().len() >= 2).await;
    assert_eq!(
        running
            .output
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e.event, RuntimeEvent::ToolAlert { .. }))
            .count(),
        1
    );
    assert_eq!(
        fixture.state.requested_cursors.lock().unwrap().as_slice(),
        &[None, Some(1)]
    );
    running.stop().await;
}

#[tokio::test]
async fn new_instance_discards_old_cursor_even_when_new_sequences_are_larger() {
    let fixture = Fixture::start(1, Some("old")).await;
    let running = Running::start(fixture.client.clone());
    until(|| {
        running
            .output
            .statuses
            .lock()
            .unwrap()
            .contains(&ConnectionStatus::Ready)
    })
    .await;
    fixture.change(2);
    until(|| running.output.events.lock().unwrap().len() == 1).await;
    fixture.state.stream_gate.block();
    fixture.close();
    until(|| fixture.state.stream_gate.count.load(Ordering::SeqCst) >= 2).await;
    *fixture.state.epoch.lock().unwrap() = Some("new".into());
    fixture.state.history.lock().unwrap().clear();
    fixture.state.sequence.store(10, Ordering::SeqCst);
    fixture.alert();
    fixture.change(9);
    fixture.state.stream_gate.release();
    until(|| fixture.state.requested_cursors.lock().unwrap().len() >= 3).await;
    until(|| {
        running
            .output
            .snapshots
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.value == 9)
    })
    .await;
    assert_eq!(
        fixture.state.requested_cursors.lock().unwrap().as_slice(),
        &[None, Some(1), None]
    );
    assert!(!running
        .output
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e.event, RuntimeEvent::ToolAlert { .. })));
    running.stop().await;
}

#[tokio::test]
async fn legacy_stream_without_epoch_resynchronizes_without_cross_instance_replay() {
    let fixture = Fixture::start(1, None).await;
    let running = Running::start(fixture.client.clone());
    until(|| {
        running
            .output
            .statuses
            .lock()
            .unwrap()
            .contains(&ConnectionStatus::Ready)
    })
    .await;
    fixture.change(2);
    until(|| running.output.events.lock().unwrap().len() == 1).await;
    fixture.state.stream_gate.block();
    fixture.close();
    until(|| fixture.state.stream_gate.count.load(Ordering::SeqCst) >= 2).await;
    fixture.alert();
    fixture.state.stream_gate.release();
    until(|| !running.output.resyncs.lock().unwrap().is_empty()).await;
    assert_eq!(
        fixture.state.requested_cursors.lock().unwrap().as_slice(),
        &[None, None]
    );
    assert_eq!(running.output.events.lock().unwrap().len(), 1);
    running.stop().await;
}

#[tokio::test]
async fn replay_gap_reloads_snapshot_and_notifies_all_read_models() {
    let fixture = Fixture::start(1, Some("same")).await;
    let running = Running::start(fixture.client.clone());
    until(|| {
        running
            .output
            .statuses
            .lock()
            .unwrap()
            .contains(&ConnectionStatus::Ready)
    })
    .await;
    fixture.state.value.store(7, Ordering::SeqCst);
    let _ = fixture.state.events.send(Wire::Resync);
    until(|| {
        running
            .output
            .resyncs
            .lock()
            .unwrap()
            .contains(&"replay-gap".into())
    })
    .await;
    assert_eq!(
        running
            .output
            .snapshots
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .value,
        7
    );
    running.stop().await;
}

#[tokio::test]
async fn shutdown_cancels_a_refresh_after_the_connection_was_ready() {
    let fixture = Fixture::start(1, Some("same")).await;
    let running = Running::start(fixture.client.clone());
    until(|| {
        running
            .output
            .statuses
            .lock()
            .unwrap()
            .contains(&ConnectionStatus::Ready)
    })
    .await;
    fixture.state.snapshot_gate.block();
    fixture.change(2);
    until(|| fixture.state.snapshot_gate.count.load(Ordering::SeqCst) >= 2).await;
    let output = running.output.clone();
    running.stop().await;
    fixture.state.snapshot_gate.release();
    assert!(output
        .snapshots
        .lock()
        .unwrap()
        .iter()
        .all(|snapshot| snapshot.value == 1));
    assert!(output.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn changing_configuration_interrupts_backoff_without_waiting_for_retry() {
    let good = Fixture::start(2, Some("good")).await;
    let state = ClientState::default();
    let output = Arc::new(Output::default());
    let (stop, rx) = watch::channel(false);
    let session = SnapshotSession::new(state.clone(), Reader, output.clone())
        .with_options(SessionOptions {
            refresh_interval: Duration::from_secs(60),
            initial_retry_delay: Duration::from_secs(10),
            max_retry_delay: Duration::from_secs(10),
        })
        .unwrap();
    let task = tokio::spawn(async move {
        session.run(rx).await;
    });
    until(|| {
        output
            .statuses
            .lock()
            .unwrap()
            .contains(&ConnectionStatus::Reconnecting)
    })
    .await;
    state.install(good.client.clone());
    until(|| {
        output
            .statuses
            .lock()
            .unwrap()
            .contains(&ConnectionStatus::Ready)
    })
    .await;
    stop.send(true).unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn closed_shutdown_channel_does_not_start_a_connection() {
    let fixture = Fixture::start(1, Some("same")).await;
    let state = ClientState::default();
    state.install(fixture.client.clone());
    let output = Arc::new(Output::default());
    let (stop, rx) = watch::channel(false);
    drop(stop);
    SnapshotSession::new(state, Reader, output.clone())
        .run(rx)
        .await;
    assert_eq!(
        fixture.state.capability_gate.count.load(Ordering::SeqCst),
        0
    );
    assert_eq!(
        *output.statuses.lock().unwrap(),
        vec![ConnectionStatus::Stopped]
    );
}
