use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;
use tokio::sync::{broadcast, watch};

pub const DEFAULT_EVENT_REPLAY_CAPACITY: usize = 256;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RuntimeEvent {
    TrackingDataChanged {
        reason: String,
        changed_at_ms: u64,
    },
    ToolsRuntimeChanged {
        changed_at_ms: u64,
    },
    ToolAlert {
        alert: crate::domain::tools::ToolAlert,
    },
}

impl RuntimeEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::TrackingDataChanged { .. } => "tracking-data-changed",
            Self::ToolsRuntimeChanged { .. } => "tools-runtime-changed",
            Self::ToolAlert { .. } => "tool-alert",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimeEventEnvelope {
    pub sequence: u64,
    pub event: RuntimeEvent,
}

#[derive(Debug)]
pub struct RuntimeEventSubscription {
    pub replay: Vec<RuntimeEventEnvelope>,
    pub resync_required: bool,
    pub receiver: broadcast::Receiver<RuntimeEventEnvelope>,
    pub shutdown: watch::Receiver<bool>,
}

#[derive(Debug)]
pub struct RuntimeEventHub {
    replay_capacity: usize,
    state: Mutex<RuntimeEventHubState>,
    sender: broadcast::Sender<RuntimeEventEnvelope>,
    shutdown_tx: watch::Sender<bool>,
}

#[derive(Debug)]
struct RuntimeEventHubState {
    next_sequence: u64,
    replay: VecDeque<RuntimeEventEnvelope>,
    stopped: bool,
}

impl RuntimeEventHub {
    pub fn new(replay_capacity: usize) -> Self {
        let replay_capacity = replay_capacity.max(1);
        let (sender, _) = broadcast::channel(replay_capacity);
        let (shutdown_tx, _) = watch::channel(false);
        Self {
            replay_capacity,
            state: Mutex::new(RuntimeEventHubState {
                next_sequence: 1,
                replay: VecDeque::with_capacity(replay_capacity),
                stopped: false,
            }),
            sender,
            shutdown_tx,
        }
    }

    pub fn subscribe_after(&self, after_sequence: Option<u64>) -> RuntimeEventSubscription {
        let state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let receiver = self.sender.subscribe();
        let latest_sequence = state.next_sequence.saturating_sub(1);
        let resync_required = after_sequence.is_some_and(|cursor| {
            if cursor > latest_sequence {
                return true;
            }
            state
                .replay
                .front()
                .is_some_and(|oldest| cursor.saturating_add(1) < oldest.sequence)
        });
        let replay = if resync_required {
            Vec::new()
        } else {
            after_sequence
                .map(|cursor| {
                    state
                        .replay
                        .iter()
                        .filter(|event| event.sequence > cursor)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        };

        RuntimeEventSubscription {
            replay,
            resync_required,
            receiver,
            shutdown: self.shutdown_tx.subscribe(),
        }
    }

    pub fn shutdown(&self) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        if state.stopped {
            return;
        }
        state.stopped = true;
        let _ = self.shutdown_tx.send(true);
    }
}

pub trait RuntimeEventSink: Send + Sync {
    fn emit(&self, event: RuntimeEvent) -> Result<(), String>;
}

impl RuntimeEventSink for RuntimeEventHub {
    fn emit(&self, event: RuntimeEvent) -> Result<(), String> {
        let envelope = {
            let mut state = match self.state.lock() {
                Ok(state) => state,
                Err(poisoned) => poisoned.into_inner(),
            };
            if state.stopped {
                return Err("runtime event hub is shut down".to_string());
            }
            let sequence = state.next_sequence;
            state.next_sequence = state
                .next_sequence
                .checked_add(1)
                .ok_or_else(|| "runtime event sequence exhausted".to_string())?;
            let envelope = RuntimeEventEnvelope { sequence, event };
            if state.replay.len() == self.replay_capacity {
                state.replay.pop_front();
            }
            state.replay.push_back(envelope.clone());
            envelope
        };
        let _ = self.sender.send(envelope);
        Ok(())
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
pub struct MemoryRuntimeEventSink {
    events: std::sync::Mutex<Vec<RuntimeEvent>>,
}

#[cfg(test)]
impl MemoryRuntimeEventSink {
    pub fn events(&self) -> Vec<RuntimeEvent> {
        match self.events.lock() {
            Ok(events) => events.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

#[cfg(test)]
impl RuntimeEventSink for MemoryRuntimeEventSink {
    fn emit(&self, event: RuntimeEvent) -> Result<(), String> {
        match self.events.lock() {
            Ok(mut events) => events.push(event),
            Err(poisoned) => poisoned.into_inner().push(event),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_sink_records_typed_events_in_order() {
        let sink = MemoryRuntimeEventSink::default();
        let first = RuntimeEvent::TrackingDataChanged {
            reason: "window-changed".to_string(),
            changed_at_ms: 1_000,
        };
        let second = RuntimeEvent::TrackingDataChanged {
            reason: "watchdog-sealed".to_string(),
            changed_at_ms: 2_000,
        };

        sink.emit(first.clone()).unwrap();
        sink.emit(second.clone()).unwrap();

        assert_eq!(sink.events(), vec![first, second]);
    }

    #[test]
    fn event_hub_assigns_monotonic_sequences() {
        let hub = RuntimeEventHub::new(8);

        hub.emit(RuntimeEvent::TrackingDataChanged {
            reason: "first".to_string(),
            changed_at_ms: 1_000,
        })
        .unwrap();
        hub.emit(RuntimeEvent::TrackingDataChanged {
            reason: "second".to_string(),
            changed_at_ms: 2_000,
        })
        .unwrap();

        let subscription = hub.subscribe_after(Some(0));
        assert_eq!(
            subscription
                .replay
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(!subscription.resync_required);
    }

    #[test]
    fn event_hub_replays_only_events_after_cursor() {
        let hub = RuntimeEventHub::new(8);
        for sequence in 1..=3 {
            hub.emit(RuntimeEvent::TrackingDataChanged {
                reason: format!("event-{sequence}"),
                changed_at_ms: sequence * 1_000,
            })
            .unwrap();
        }

        let subscription = hub.subscribe_after(Some(1));
        assert_eq!(
            subscription
                .replay
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert!(!subscription.resync_required);
    }

    #[test]
    fn event_hub_requires_resync_when_cursor_falls_outside_bounded_replay() {
        let hub = RuntimeEventHub::new(2);
        for sequence in 1..=3 {
            hub.emit(RuntimeEvent::TrackingDataChanged {
                reason: format!("event-{sequence}"),
                changed_at_ms: sequence * 1_000,
            })
            .unwrap();
        }

        let too_old = hub.subscribe_after(Some(0));
        assert!(too_old.resync_required);
        assert!(too_old.replay.is_empty());

        let from_previous_process = hub.subscribe_after(Some(99));
        assert!(from_previous_process.resync_required);
        assert!(from_previous_process.replay.is_empty());
    }

    #[tokio::test]
    async fn event_hub_shutdown_notifies_existing_subscriptions() {
        let hub = RuntimeEventHub::new(2);
        let mut subscription = hub.subscribe_after(None);

        hub.shutdown();

        subscription.shutdown.changed().await.unwrap();
        assert!(*subscription.shutdown.borrow());
        assert!(hub
            .emit(RuntimeEvent::TrackingDataChanged {
                reason: "after-shutdown".to_string(),
                changed_at_ms: 1_000,
            })
            .is_err());
    }
}
