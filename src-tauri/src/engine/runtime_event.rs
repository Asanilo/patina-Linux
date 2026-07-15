#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeEvent {
    TrackingDataChanged { reason: String, changed_at_ms: u64 },
}

pub trait RuntimeEventSink: Send + Sync {
    fn emit(&self, event: RuntimeEvent) -> Result<(), String>;
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
}
