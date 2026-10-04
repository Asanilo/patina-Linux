//! Real Tools owner and authenticated HTTP. No desktop notification or production profile.
use super::{
    auth::ApiCredentialStore,
    context::{ApiRuntimeContext, ApiRuntimeStateProvider},
    server::prepare_standalone_server_with_events,
    surface::ApiSurface,
};
use crate::domain::tools as domain;
use crate::engine::{
    runtime_context::{RuntimeClock, RuntimeContext},
    runtime_event::{RuntimeEvent, RuntimeEventHub, RuntimeEventSink},
    tools::{ToolsRuntimeOwner, ToolsRuntimeSink},
};
use crate::platform::daemon_client::PatinadClient;
use patina_protocol::tools::*;
use sqlx::Executor;
use std::{
    sync::{
        atomic::{AtomicI64, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

struct Clock(AtomicI64);
impl RuntimeClock for Clock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}
struct Ready;
impl ApiRuntimeStateProvider for Ready {
    fn tracking_snapshot(
        &self,
    ) -> Option<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshot> {
        None
    }
    fn web_activity_snapshot(
        &self,
        _: &crate::domain::settings::WebActivitySettings,
        _: i64,
    ) -> Option<crate::domain::web_activity::WebActivityBridgeSnapshot> {
        None
    }
    fn tools_runtime_ready(&self) -> bool {
        true
    }
}
struct Sink {
    hub: Arc<RuntimeEventHub>,
    alerts: AtomicUsize,
    snapshots: AtomicUsize,
}
impl ToolsRuntimeSink for Sink {
    fn snapshot_changed(&self, snapshot: &domain::ToolsRuntimeSnapshot) {
        self.snapshots.fetch_add(1, Ordering::SeqCst);
        self.hub
            .emit(RuntimeEvent::ToolsRuntimeChanged {
                changed_at_ms: snapshot.sampled_at_ms as u64,
            })
            .unwrap();
    }
    fn alert(&self, alert: &domain::ToolAlert) {
        self.alerts.fetch_add(1, Ordering::SeqCst);
        self.hub
            .emit(RuntimeEvent::ToolAlert {
                alert: alert.clone(),
            })
            .unwrap();
    }
}

#[tokio::test]
async fn independent_tools_clients_share_owner_state_and_do_not_replay_alerts() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for schema in [
        crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL,
        crate::data::schema::TOOLS_TABLES_SCHEMA_SQL,
        crate::data::schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL,
    ] {
        pool.execute(schema).await.unwrap();
    }
    let clock = Arc::new(Clock(AtomicI64::new(1_782_000_000_000)));
    let runtime = RuntimeContext::new(pool.clone(), clock.clone());
    let hub = Arc::new(RuntimeEventHub::new(64));
    let sink = Arc::new(Sink {
        hub: hub.clone(),
        alerts: AtomicUsize::new(0),
        snapshots: AtomicUsize::new(0),
    });
    let owner = Arc::new(ToolsRuntimeOwner::new(runtime.clone(), sink.clone()));
    let (stop_worker, worker_shutdown) = tokio::sync::watch::channel(false);
    let worker_owner = owner.clone();
    let worker = tokio::spawn(async move { worker_owner.run_with_shutdown(worker_shutdown).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while sink.snapshots.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let context = ApiRuntimeContext::with_state_and_events(
        runtime,
        "1.9.2",
        "linux",
        Arc::new(Ready),
        Some(hub.clone()),
    )
    .with_tools_owner(owner.clone());
    let credentials = ApiCredentialStore::new();
    let path = std::env::temp_dir().join(format!(
        "patina-tools-contract-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    credentials
        .initialize_at(&path, Some("tools-contract"))
        .unwrap();
    std::fs::remove_file(path).unwrap();
    let server = prepare_standalone_server_with_events(
        0,
        credentials,
        context,
        ApiSurface::DaemonTracking,
        hub,
    )
    .await
    .unwrap();
    let port = server.port();
    let shutdown = server.shutdown_handle();
    let task = tokio::spawn(server.run());
    let first = patina_client::Client::new(port, "tools-contract").unwrap();
    let second = patina_client::Client::new(port, "tools-contract").unwrap();
    let desktop = PatinadClient::new(port, "tools-contract").unwrap();
    let mut events = second.open_event_stream(None).await.unwrap();
    let snapshot = first
        .start_timer(&StartTimerRequest {
            mode: TimerMode::Stopwatch,
            duration_ms: None,
            label: Some("Shared".into()),
        })
        .await
        .unwrap();
    let changed = tokio::time::timeout(Duration::from_secs(3), events.next_event())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(changed.event, "tools-runtime-changed");
    assert_eq!(snapshot, second.tools_snapshot().await.unwrap());
    assert_eq!(snapshot, desktop.tools_snapshot().await.unwrap().into());
    // Round-trip conversion is lossless while clock/phase methods remain backend-owned.
    let raw = serde_json::to_value(&snapshot).unwrap();
    let as_domain: domain::ToolsRuntimeSnapshot = snapshot.into();
    assert_eq!(raw, serde_json::to_value(&as_domain).unwrap());
    clock.0.fetch_add(1500, Ordering::SeqCst);
    let lapped = second.tools_action(ToolsAction::AddTimerLap).await.unwrap();
    assert_eq!(lapped.timer_laps.len(), 1);
    assert_eq!(lapped.timer_laps[0].duration_ms, 1500);
    assert_eq!(lapped, desktop.tools_snapshot().await.unwrap().into());
    let mut concurrent = Vec::new();
    for index in 0..8 {
        let client = if index % 2 == 0 {
            first.clone()
        } else {
            second.clone()
        };
        concurrent.push(tokio::spawn(async move {
            client
                .tools_action(ToolsAction::AddTimerLap)
                .await
                .unwrap()
                .timer_laps
                .len()
        }));
    }
    let mut confirmed_counts = Vec::new();
    for task in concurrent {
        confirmed_counts.push(task.await.unwrap());
    }
    confirmed_counts.sort();
    assert_eq!(confirmed_counts, (2..=9).collect::<Vec<_>>());
    let concurrent_snapshot = first.tools_snapshot().await.unwrap();
    assert_eq!(
        concurrent_snapshot
            .timer_laps
            .iter()
            .map(|lap| lap.lap_index)
            .collect::<Vec<_>>(),
        (1..=9).collect::<Vec<_>>()
    );
    assert_eq!(
        concurrent_snapshot
            .timer_laps
            .iter()
            .map(|lap| lap.duration_ms)
            .sum::<i64>(),
        1500
    );
    let paused: ToolsRuntimeSnapshot = desktop
        .tools_action(ToolsAction::PauseTimer)
        .await
        .unwrap()
        .into();
    assert_eq!(
        paused.current_timer.as_ref().unwrap().status,
        TimerStatus::Paused
    );
    assert_eq!(paused, first.tools_snapshot().await.unwrap());
    first.tools_action(ToolsAction::ResumeTimer).await.unwrap();
    first.tools_action(ToolsAction::ResetTimer).await.unwrap();
    let rule = first
        .create_software_reminder_rule(&CreateSoftwareReminderRuleRequest {
            app_name: "Editor".into(),
            exe_name: Some("editor".into()),
            limit_ms: 60000,
            message: "Rest".into(),
        })
        .await
        .unwrap();
    assert_eq!(rule, desktop.tools_snapshot().await.unwrap().into());
    second
        .disable_software_reminder_rule(rule.software_reminder_rules[0].id)
        .await
        .unwrap();
    let pomodoro = first
        .start_pomodoro(&StartPomodoroRequest {
            focus_ms: 60000,
            short_break_ms: 60000,
            long_break_ms: 120000,
            long_break_every: 2,
        })
        .await
        .unwrap();
    assert_eq!(
        pomodoro.current_pomodoro.as_ref().unwrap().phase,
        PomodoroPhase::Focus
    );
    assert_eq!(pomodoro, desktop.tools_snapshot().await.unwrap().into());
    second
        .tools_action(ToolsAction::PausePomodoro)
        .await
        .unwrap();
    second
        .tools_action(ToolsAction::ResumePomodoro)
        .await
        .unwrap();
    let skipped = second
        .tools_action(ToolsAction::SkipPomodoroPhase)
        .await
        .unwrap();
    assert_eq!(
        skipped.current_pomodoro.as_ref().unwrap().phase,
        PomodoroPhase::ShortBreak
    );
    second
        .tools_action(ToolsAction::ResetPomodoro)
        .await
        .unwrap();
    let reminder = first
        .create_reminder(&CreateReminderRequest {
            label: "Cancelled".into(),
            scheduled_at: clock.now_ms() + 1000,
        })
        .await
        .unwrap();
    second
        .cancel_reminder(reminder.reminders[0].id)
        .await
        .unwrap();
    first
        .create_reminder(&CreateReminderRequest {
            label: "Once".into(),
            scheduled_at: clock.now_ms() + 1000,
        })
        .await
        .unwrap();
    let alerts_before = sink.alerts.load(Ordering::SeqCst);
    clock.0.fetch_add(1001, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(3), async {
        while sink.alerts.load(Ordering::SeqCst) == alerts_before {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(sink.alerts.load(Ordering::SeqCst), alerts_before + 1);
    let final_snapshot = first.tools_snapshot().await.unwrap();
    assert_eq!(final_snapshot, second.tools_snapshot().await.unwrap());
    assert_eq!(
        final_snapshot,
        desktop.tools_snapshot().await.unwrap().into()
    );
    assert_eq!(
        final_snapshot
            .reminders
            .iter()
            .filter(|r| r.status == ReminderStatus::Fired)
            .count(),
        1
    );
    // A new live subscription starts now; a snapshot read never redelivers old alerts.
    let mut fresh = second.open_event_stream(None).await.unwrap();
    second.tools_snapshot().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(150), fresh.next_event())
            .await
            .is_err()
    );
    drop(events);
    drop(fresh);
    shutdown.shutdown();
    task.await.unwrap();
    stop_worker.send(true).unwrap();
    worker.await.unwrap().unwrap();
    pool.close().await;
}
