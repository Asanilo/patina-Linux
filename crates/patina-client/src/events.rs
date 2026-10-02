use crate::{ClientError, Event};
use patina_protocol::events::RuntimeEventEnvelope;

const MAX_EVENT_DATA_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamEvent {
    Runtime(RuntimeEventEnvelope),
    ResyncRequired {
        reason: String,
        missed: Option<u64>,
    },
    Ignored {
        event: String,
        sequence: Option<u64>,
    },
}

impl StreamEvent {
    pub fn sequence(&self) -> Option<u64> {
        match self {
            Self::Runtime(envelope) => Some(envelope.sequence),
            Self::Ignored { sequence, .. } => *sequence,
            Self::ResyncRequired { .. } => None,
        }
    }
}

pub struct RuntimeEventStream {
    pub(crate) inner: crate::EventStream,
}
impl RuntimeEventStream {
    pub fn instance_id(&self) -> Option<&str> {
        self.inner.instance_id()
    }
    pub async fn next_event(&mut self) -> Result<Option<StreamEvent>, ClientError> {
        self.inner
            .next_event()
            .await?
            .map(parse_stream_event)
            .transpose()
    }
}
pub fn parse_stream_event(event: Event) -> Result<StreamEvent, ClientError> {
    if event.data.len() > MAX_EVENT_DATA_BYTES {
        return Err(ClientError::ResponseTooLarge);
    }
    if event.event == "resync-required" {
        #[derive(serde::Deserialize)]
        struct ResyncPayload {
            reason: String,
            missed: Option<u64>,
        }
        let payload = serde_json::from_str::<ResyncPayload>(&event.data).map_err(|error| {
            ClientError::InvalidResponse(format!("failed to decode patinad resync event: {error}"))
        })?;
        return Ok(StreamEvent::ResyncRequired {
            reason: payload.reason,
            missed: payload.missed,
        });
    }

    let sequence = parse_optional_event_sequence(&event.id)?;
    let is_known_runtime_event = matches!(
        event.event.as_str(),
        "tracking-data-changed"
            | "scheduled-backup-changed"
            | "tools-runtime-changed"
            | "tool-alert"
    );
    if !is_known_runtime_event {
        return Ok(StreamEvent::Ignored {
            event: event.event,
            sequence,
        });
    }

    let envelope = serde_json::from_str::<RuntimeEventEnvelope>(&event.data).map_err(|error| {
        ClientError::InvalidResponse(format!("failed to decode patinad runtime event: {error}"))
    })?;
    let Some(sequence) = sequence else {
        return Err(ClientError::InvalidResponse(
            "patinad runtime event is missing its sequence ID".to_string(),
        ));
    };
    if envelope.sequence != sequence || envelope.event.event_name() != event.event {
        return Err(ClientError::InvalidResponse(
            "patinad runtime event ID or type does not match its envelope".to_string(),
        ));
    }
    Ok(StreamEvent::Runtime(envelope))
}

fn parse_optional_event_sequence(value: &str) -> Result<Option<u64>, ClientError> {
    if value.is_empty() {
        return Ok(None);
    }
    value.parse::<u64>().map(Some).map_err(|_| {
        ClientError::InvalidResponse(
            "patinad event stream returned an invalid sequence ID".to_string(),
        )
    })
}
