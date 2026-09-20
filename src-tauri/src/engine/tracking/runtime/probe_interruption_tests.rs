use super::window_polling::{poll_active_window_with_state, ForegroundProbeState};
use super::*;
use crate::data::schema;
use crate::domain::{settings::WebActivitySettings, web_activity::WebActivityBridgeSnapshot};
use crate::engine::api::context::{ApiRuntimeContext, ApiRuntimeStateProvider};
use crate::engine::runtime_context::RuntimeClock;
use crate::engine::runtime_event::MemoryRuntimeEventSink;
use crate::engine::tracking::runtime_snapshot::{
    TrackingRuntimeProbeDiagnostics, TrackingRuntimeProbeStatus,
};
use sqlx::{sqlite::SqlitePoolOptions, Executor, SqlitePool};
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::sync::{mpsc, Mutex};

#[derive(Default)]
struct TestClock(AtomicI64);

impl RuntimeClock for TestClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct Sample {
    at_ms: i64,
    outcome: WindowPollOutcome,
}

struct LoopFixture {
    pool: SqlitePool,
    clock: Arc<TestClock>,
    context: RuntimeContext,
    state: Arc<TrackingRuntimeSnapshotState>,
    health: Arc<watchdog::RuntimeHealthState>,
    events: Arc<MemoryRuntimeEventSink>,
    samples: mpsc::UnboundedSender<Sample>,
    ready: mpsc::UnboundedReceiver<()>,
    shutdown: watch::Sender<bool>,
    task: tokio::task::JoinHandle<Result<(), String>>,
}

impl LoopFixture {
    async fn start() -> Self {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for ddl in [
            schema::CURRENT_BASELINE_SCHEMA_SQL,
            schema::WEB_ACTIVITY_SCHEMA_SQL,
            schema::WEB_ACTIVITY_SESSION_SCHEMA_SQL,
        ] {
            pool.execute(ddl).await.unwrap();
        }
        // Prevent asynchronous metadata work from querying the desktop's icons.
        pool.execute(
            "INSERT INTO icon_cache VALUES ('zen', 'synthetic', 0), ('editor', 'synthetic', 0)",
        )
        .await
        .unwrap();
        let clock = Arc::new(TestClock::default());
        let context = RuntimeContext::new(pool.clone(), clock.clone());
        let state = Arc::new(TrackingRuntimeSnapshotState::default());
        let health = Arc::new(watchdog::RuntimeHealthState::default());
        let events = Arc::new(MemoryRuntimeEventSink::default());
        let (samples, rx) = mpsc::unbounded_channel::<Sample>();
        let rx = Arc::new(Mutex::new(rx));
        let (ready_tx, ready) = mpsc::unbounded_channel();
        let (shutdown, shutdown_rx) = watch::channel(false);
        let probe_clock = clock.clone();
        let task = tokio::spawn(run_with_probe(
            context.clone(),
            health.clone(),
            events.clone(),
            state.clone(),
            (
                crate::platform::linux::audio::AudioSignalSource::new(false),
                crate::platform::linux::media::MediaSignalSource::new(),
            ),
            shutdown_rx,
            move || {
                let rx = rx.clone();
                let clock = probe_clock.clone();
                let ready = ready_tx.clone();
                async move {
                    // Polling the next sample proves the previous loop iteration
                    // completed, including its transactions and health updates.
                    ready.send(()).unwrap();
                    let sample = rx.lock().await.recv().await.unwrap();
                    clock.0.store(sample.at_ms, Ordering::SeqCst);
                    sample.outcome
                }
            },
        ));
        let mut fixture = Self {
            pool,
            clock,
            context,
            state,
            health,
            events,
            samples,
            ready,
            shutdown,
            task,
        };
        fixture.wait_ready().await;
        fixture
    }

    async fn wait_ready(&mut self) {
        tokio::time::timeout(Duration::from_secs(5), self.ready.recv())
            .await
            .unwrap()
            .unwrap();
    }

    async fn sample(&mut self, at_ms: i64, app: &str, successful: bool) {
        self.sample_outcome(
            at_ms,
            WindowPollOutcome {
                window: window(app),
                probe_status: if successful {
                    TrackingRuntimeProbeStatus::Ok
                } else {
                    TrackingRuntimeProbeStatus::TimeoutFallback
                },
                degraded_reason: (!successful).then(|| "synthetic timeout".into()),
                probe_diagnostics: TrackingRuntimeProbeDiagnostics::default(),
            },
        )
        .await;
    }

    async fn sample_outcome(&mut self, at_ms: i64, outcome: WindowPollOutcome) {
        self.samples.send(Sample { at_ms, outcome }).unwrap();
        self.wait_ready().await;
    }

    async fn seed_active_browser(&self) {
        self.pool.execute(
            "INSERT INTO web_activity_segments
             (browser_client_id, browser_kind, browser_exe_name, domain, normalized_domain,
              start_time, created_at, updated_at)
             VALUES ('synthetic', 'firefox', 'zen', 'example.test', 'example.test', 1500, 1500, 1500);
             INSERT INTO web_activity_native_sessions (segment_id, session_id) VALUES (1, 1);"
        ).await.unwrap();
    }

    async fn reject_seals(&self) {
        self.pool
            .execute(
                "CREATE TRIGGER reject_probe_seal BEFORE UPDATE OF end_time ON sessions
             WHEN NEW.end_time IS NOT NULL
             BEGIN SELECT RAISE(FAIL, 'synthetic seal failure'); END;",
            )
            .await
            .unwrap();
    }

    async fn allow_seals(&self) {
        self.pool
            .execute("DROP TRIGGER reject_probe_seal")
            .await
            .unwrap();
    }

    async fn persisted_sample(&self) -> Option<i64> {
        TrackingRuntimeDataStore::new(self.pool.clone())
            .load_tracker_successful_sample_timestamp()
            .await
            .unwrap()
    }

    async fn rows(&self) -> Vec<(String, i64, Option<i64>, i64)> {
        sqlx::query_as("SELECT exe_name, start_time, end_time, continuity_group_start_time FROM sessions ORDER BY id")
            .fetch_all(&self.pool).await.unwrap()
    }

    fn interruption_events(&self) -> Vec<u64> {
        self.events
            .events()
            .iter()
            .filter_map(|event| match event {
                RuntimeEvent::TrackingDataChanged {
                    reason,
                    changed_at_ms,
                } if reason == "session-ended-probe-failure" => Some(*changed_at_ms),
                _ => None,
            })
            .collect()
    }

    async fn stop(self) {
        self.shutdown.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(5), self.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        self.pool.close().await;
    }
}

fn window(app: &str) -> tracker::WindowInfo {
    tracker::WindowInfo {
        hwnd: if app.is_empty() {
            ""
        } else {
            "synthetic-window"
        }
        .into(),
        root_owner_hwnd: String::new(),
        process_id: 0,
        window_class: String::new(),
        title: if app.is_empty() {
            ""
        } else {
            "Synthetic title"
        }
        .into(),
        exe_name: app.into(),
        process_path: String::new(),
        is_afk: false,
        idle_time_ms: 0,
    }
}

#[tokio::test]
async fn short_interruption_splits_same_app_title_and_web_in_the_real_loop() {
    let mut f = LoopFixture::start().await;
    f.sample(1_000, "zen", true).await;
    f.seed_active_browser().await;
    f.sample(2_000, "zen", true).await;
    // The persisted heartbeat cadence is deliberately older than the trusted sample.
    assert_eq!(f.persisted_sample().await, Some(1_000));
    f.sample(4_000, "zen", false).await;
    assert!(!f.state.snapshot().unwrap().status.is_tracking_active);
    f.sample(4_500, "zen", false).await;
    assert_eq!(f.interruption_events(), vec![2_000]);
    f.sample(6_000, "zen", true).await;
    f.sample(8_000, "", true).await;
    assert_eq!(
        f.rows().await,
        vec![
            ("zen".into(), 1_000, Some(2_000), 1_000),
            ("zen".into(), 6_000, Some(8_000), 6_000),
        ]
    );
    let titles: Vec<(i64, Option<i64>)> =
        sqlx::query_as("SELECT start_time, end_time FROM session_title_samples ORDER BY id")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert_eq!(titles, vec![(1_000, Some(2_000)), (6_000, Some(8_000))]);
    let web: (i64, Option<i64>, i64) =
        sqlx::query_as("SELECT start_time, end_time, duration FROM web_activity_segments")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(web, (1_500, Some(2_000), 500));
    f.stop().await;
}

#[tokio::test]
async fn idle_provider_failure_from_poller_seals_and_recovers_without_bridging_the_gap() {
    let mut f = LoopFixture::start().await;
    let probe_state = Arc::new(ForegroundProbeState::default());
    for at_ms in [1_000, 2_000] {
        let outcome = poll_active_window_with_state(
            probe_state.clone(),
            Duration::from_secs(1),
            at_ms,
            || Ok(window("zen")),
        )
        .await;
        f.sample_outcome(at_ms, outcome).await;
        if at_ms == 1_000 {
            f.seed_active_browser().await;
        }
    }

    for at_ms in [4_000, 4_500] {
        // Exercise the real poller's Result handling and the Linux error's
        // diagnostic code without contacting a desktop provider.
        let outcome = poll_active_window_with_state(
            probe_state.clone(),
            Duration::from_secs(1),
            at_ms,
            || Err(tracker::ForegroundProbeError::IdleUnavailable.to_string()),
        )
        .await;
        assert_eq!(outcome.window.exe_name, "zen");
        assert_eq!(
            outcome.probe_status,
            TrackingRuntimeProbeStatus::TaskFailedFallback
        );
        assert!(!outcome.is_successful_sample());
        assert_eq!(
            outcome.probe_diagnostics.last_successful_sample_at_ms,
            Some(2_000)
        );
        f.sample_outcome(at_ms, outcome).await;

        let snapshot = f.state.snapshot().unwrap();
        assert!(!snapshot.status.is_tracking_active);
        assert_eq!(
            snapshot.probe_status,
            TrackingRuntimeProbeStatus::TaskFailedFallback
        );
        assert_eq!(
            snapshot.degraded_reason.as_deref(),
            Some("active window provider failed: linux-idle-unavailable")
        );
        assert_eq!(f.health.snapshot().last_successful_sample_ms, Some(2_000));
        assert_eq!(f.persisted_sample().await, Some(1_000));
        assert_eq!(
            f.rows().await,
            vec![("zen".into(), 1_000, Some(2_000), 1_000)]
        );
        assert_eq!(f.interruption_events(), vec![2_000]);
    }

    for (at_ms, app) in [(6_000, "zen"), (8_000, "")] {
        let outcome = poll_active_window_with_state(
            probe_state.clone(),
            Duration::from_secs(1),
            at_ms,
            move || Ok(window(app)),
        )
        .await;
        assert!(outcome.is_successful_sample());
        assert_eq!(outcome.probe_diagnostics.consecutive_fallback_count, 0);
        f.sample_outcome(at_ms, outcome).await;
    }
    assert_eq!(
        f.rows().await,
        vec![
            ("zen".into(), 1_000, Some(2_000), 1_000),
            ("zen".into(), 6_000, Some(8_000), 6_000),
        ]
    );
    let titles: Vec<(i64, Option<i64>)> =
        sqlx::query_as("SELECT start_time, end_time FROM session_title_samples ORDER BY id")
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert_eq!(titles, vec![(1_000, Some(2_000)), (6_000, Some(8_000))]);
    let web: (i64, Option<i64>, i64) =
        sqlx::query_as("SELECT start_time, end_time, duration FROM web_activity_segments")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(web, (1_500, Some(2_000), 500));
    f.stop().await;
}

#[tokio::test]
async fn failed_seal_blocks_recovered_sample_timestamps_until_the_same_boundary_commits() {
    let mut f = LoopFixture::start().await;
    f.pool.execute(
        r#"INSERT INTO settings VALUES ('__app_override::zen.exe', '{"captureTitle":false,"enabled":true}')"#,
    ).await.unwrap();
    f.sample(1_000, "zen", true).await;
    f.sample(2_000, "zen", true).await;
    f.reject_seals().await;
    f.sample(4_000, "zen", false).await;
    f.sample(6_000, "zen", true).await;
    assert_eq!(f.state.pending_probe_seal(), Some(2_000));
    assert_eq!(f.health.snapshot().last_successful_sample_ms, Some(2_000));
    assert_eq!(f.persisted_sample().await, Some(1_000));
    assert_eq!(f.rows().await, vec![("zen".into(), 1_000, None, 1_000)]);
    let blocked_snapshot = f.state.snapshot().unwrap();
    assert!(!blocked_snapshot.status.is_tracking_active);
    assert_eq!(
        blocked_snapshot.probe_status,
        TrackingRuntimeProbeStatus::TimeoutFallback
    );
    assert_eq!(
        blocked_snapshot.degraded_reason.as_deref(),
        Some("synthetic timeout")
    );
    assert!(blocked_snapshot.window.title.is_empty());
    assert!(f.interruption_events().is_empty());
    f.allow_seals().await;
    f.sample(7_000, "zen", true).await;
    assert_eq!(
        f.rows().await,
        vec![
            ("zen".into(), 1_000, Some(2_000), 1_000),
            ("zen".into(), 7_000, None, 7_000),
        ]
    );
    assert_eq!(f.persisted_sample().await, Some(7_000));
    assert_eq!(f.interruption_events(), vec![2_000]);
    assert_eq!(f.state.pending_probe_seal(), None);
    f.stop().await;
}

#[tokio::test]
async fn initial_failure_and_recovery_without_a_window_do_not_create_or_bridge_sessions() {
    let mut f = LoopFixture::start().await;
    f.sample(1_000, "zen", false).await;
    assert!(f.rows().await.is_empty());
    assert_eq!(f.health.snapshot().last_successful_sample_ms, None);
    f.sample(2_000, "zen", true).await;
    f.sample(3_000, "zen", false).await;
    f.sample(4_000, "", true).await;
    f.sample(5_000, "editor", true).await;
    assert_eq!(
        f.rows().await,
        vec![
            ("zen".into(), 2_000, Some(2_000), 2_000),
            ("editor".into(), 5_000, None, 5_000),
        ]
    );
    f.stop().await;
}

#[tokio::test]
async fn shutdown_retries_the_pending_boundary_without_accepting_a_new_sample() {
    let mut f = LoopFixture::start().await;
    f.sample(1_000, "zen", true).await;
    f.sample(2_000, "zen", true).await;
    f.reject_seals().await;
    f.sample(4_000, "zen", false).await;
    f.allow_seals().await;
    f.clock.0.store(9_000, Ordering::SeqCst);
    f.shutdown.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(5), &mut f.task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        f.rows().await,
        vec![("zen".into(), 1_000, Some(2_000), 1_000)]
    );
    assert_eq!(f.health.snapshot().last_successful_sample_ms, Some(2_000));
    assert_eq!(f.state.pending_probe_seal(), None);
    f.pool.close().await;
}

#[tokio::test]
async fn later_lock_uses_the_pending_probe_boundary_after_a_failed_loop_seal() {
    let mut f = LoopFixture::start().await;
    f.sample(1_000, "zen", true).await;
    f.seed_active_browser().await;
    f.sample(2_000, "zen", true).await;
    f.reject_seals().await;
    f.sample(4_000, "zen", false).await;
    assert!(handle_power_lifecycle_event_with_context(
        &f.context,
        f.events.as_ref(),
        f.state.as_ref(),
        "lock",
        8_000,
    )
    .await
    .is_err());
    assert_eq!(f.state.pending_probe_seal(), Some(2_000));
    f.allow_seals().await;
    flush_noted_power_lifecycle_event(&f.context, f.events.as_ref(), f.state.as_ref())
        .await
        .unwrap();
    assert_eq!(
        f.rows().await,
        vec![("zen".into(), 1_000, Some(2_000), 1_000)]
    );
    assert_eq!(f.state.pending_probe_seal(), None);
    assert_eq!(f.state.pending_stop(), None);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT end_time FROM web_activity_segments")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        2_000
    );
    assert!(!serde_json::to_string(&f.state.snapshot().unwrap())
        .unwrap()
        .contains("pending_probe"));
    f.stop().await;
}

struct TestApiState(TrackingRuntimeSnapshotState);

impl ApiRuntimeStateProvider for TestApiState {
    fn tracking_snapshot(&self) -> Option<TrackingRuntimeSnapshot> {
        self.0.snapshot()
    }
    fn tracking_runtime_state(&self) -> Option<TrackingRuntimeSnapshotState> {
        Some(self.0.clone())
    }
    fn web_activity_snapshot(
        &self,
        _: &WebActivitySettings,
        _: i64,
    ) -> Option<WebActivityBridgeSnapshot> {
        None
    }
    fn tools_runtime_ready(&self) -> bool {
        false
    }
}

#[tokio::test]
async fn api_pause_cannot_move_a_failed_probe_boundary_forward() {
    let mut f = LoopFixture::start().await;
    f.sample(1_000, "zen", true).await;
    f.sample(2_000, "zen", true).await;
    f.reject_seals().await;
    f.sample(4_000, "zen", false).await;
    f.clock.0.store(9_000, Ordering::SeqCst);
    let api = ApiRuntimeContext::with_state(
        f.context.clone(),
        "test",
        "linux",
        Arc::new(TestApiState((*f.state).clone())),
    );
    let body = br#"{"paused":true}"#;
    assert_eq!(
        crate::engine::api::handlers::settings::set_tracking_paused(&api, body)
            .await
            .status,
        500
    );
    assert_eq!(f.state.pending_probe_seal(), Some(2_000));
    f.allow_seals().await;
    assert_eq!(
        crate::engine::api::handlers::settings::set_tracking_paused(&api, body)
            .await
            .status,
        200
    );
    assert_eq!(
        f.rows().await,
        vec![("zen".into(), 1_000, Some(2_000), 1_000)]
    );
    assert_eq!(f.state.pending_probe_seal(), None);
    assert_eq!(f.health.snapshot().last_successful_sample_ms, Some(2_000));
    f.stop().await;
}
