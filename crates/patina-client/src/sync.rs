//! Host-independent subscription, snapshot refresh and reconnect coordination.
//! Output callbacks are synchronous and must not replace client configuration or
//! send shutdown themselves; schedule such actions after returning instead.
use crate::{
    events::StreamEvent,
    state::{ClientState, Configuration},
    Client, ClientError,
};
use futures_util::future::BoxFuture;
use patina_protocol::events::RuntimeEventEnvelope;
use serde::Serialize;
use std::{future::Future, sync::Arc, time::Duration};
use tokio::sync::watch;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectionStatus {
    Connecting,
    Ready,
    Reconnecting,
    Stopped,
}

pub trait SnapshotReader: Send + Sync {
    type Snapshot: Clone + Send + Sync + 'static;
    fn read<'a>(
        &'a self,
        client: &'a Client,
        cursor: Option<u64>,
    ) -> BoxFuture<'a, Result<Self::Snapshot, ClientError>>;
    fn needs_refresh(&self, snapshot: &Self::Snapshot, event: &RuntimeEventEnvelope) -> bool;
}

pub trait SnapshotOutput<S>: Send + Sync {
    fn connection_changed(&self, status: ConnectionStatus, error: Option<&ClientError>);
    fn snapshot_changed(&self, snapshot: S);
    fn tracking_data_changed(&self, event: &RuntimeEventEnvelope);
    fn resync_required(&self, reason: &str, missed: Option<u64>);
}

#[derive(Clone, Debug)]
pub struct SessionOptions {
    pub refresh_interval: Duration,
    pub initial_retry_delay: Duration,
    pub max_retry_delay: Duration,
}
impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            refresh_interval: Duration::from_secs(2),
            initial_retry_delay: Duration::from_millis(500),
            max_retry_delay: Duration::from_secs(10),
        }
    }
}

pub struct SnapshotSession<R: SnapshotReader> {
    state: ClientState,
    reader: R,
    output: Arc<dyn SnapshotOutput<R::Snapshot>>,
    options: SessionOptions,
}

#[derive(Default)]
struct Resume {
    cursor: Option<u64>,
    instance_id: Option<String>,
    connected_before: bool,
}
impl Resume {
    fn clear_cursor(&mut self) {
        self.cursor = None;
        self.instance_id = None;
    }
}

enum Exit {
    Shutdown,
    Reconfigure,
    Failed(ClientError),
}
impl From<ClientError> for Exit {
    fn from(error: ClientError) -> Self {
        Self::Failed(error)
    }
}

impl<R: SnapshotReader> SnapshotSession<R> {
    pub fn new(
        state: ClientState,
        reader: R,
        output: Arc<dyn SnapshotOutput<R::Snapshot>>,
    ) -> Self {
        Self {
            state,
            reader,
            output,
            options: SessionOptions::default(),
        }
    }

    pub fn with_options(mut self, options: SessionOptions) -> Result<Self, ClientError> {
        if options.refresh_interval.is_zero()
            || options.initial_retry_delay.is_zero()
            || options.max_retry_delay < options.initial_retry_delay
        {
            return Err(ClientError::InvalidConfiguration(
                "invalid snapshot session timing".into(),
            ));
        }
        self.options = options;
        Ok(self)
    }

    pub async fn run(&self, mut shutdown: watch::Receiver<bool>) {
        let mut configuration = self.state.subscribe();
        let mut resume = Resume::default();
        let mut first_attempt = true;
        let mut delay = self.options.initial_retry_delay;
        loop {
            if stopped(&shutdown) {
                self.output
                    .connection_changed(ConnectionStatus::Stopped, None);
                return;
            }
            let selected = configuration.borrow_and_update().clone();
            if publish(&configuration, selected.revision, &shutdown, || {
                self.output.connection_changed(
                    if first_attempt {
                        ConnectionStatus::Connecting
                    } else {
                        ConnectionStatus::Reconnecting
                    },
                    None,
                )
            })
            .is_err()
            {
                continue;
            }
            first_attempt = false;
            let mut became_ready = false;
            let result = self
                .run_connection(
                    &selected,
                    &mut configuration,
                    &mut shutdown,
                    &mut resume,
                    &mut became_ready,
                )
                .await;
            if became_ready {
                delay = self.options.initial_retry_delay;
            }
            match result {
                Err(Exit::Shutdown) => {
                    self.output
                        .connection_changed(ConnectionStatus::Stopped, None);
                    return;
                }
                Err(Exit::Reconfigure) => {
                    resume.clear_cursor();
                    delay = self.options.initial_retry_delay;
                    continue;
                }
                Err(Exit::Failed(error)) => {
                    match publish(&configuration, selected.revision, &shutdown, || {
                        self.output
                            .connection_changed(ConnectionStatus::Reconnecting, Some(&error))
                    }) {
                        Err(Exit::Shutdown) => {
                            self.output
                                .connection_changed(ConnectionStatus::Stopped, None);
                            return;
                        }
                        Err(Exit::Reconfigure) => {
                            resume.clear_cursor();
                            continue;
                        }
                        _ => {}
                    }
                }
                Ok(()) => unreachable!("a connection only ends with an explicit reason"),
            }
            // Reconfiguration interrupts backoff as well as in-flight HTTP/SSE.
            let wait = current(
                async {
                    tokio::time::sleep(delay).await;
                    Ok(())
                },
                &mut configuration,
                selected.revision,
                &mut shutdown,
            )
            .await;
            match wait {
                Err(Exit::Shutdown) => {
                    self.output
                        .connection_changed(ConnectionStatus::Stopped, None);
                    return;
                }
                Err(Exit::Reconfigure) => {
                    resume.clear_cursor();
                    delay = self.options.initial_retry_delay;
                }
                _ => {
                    delay = delay.saturating_mul(2).min(self.options.max_retry_delay);
                }
            }
        }
    }

    async fn run_connection(
        &self,
        selected: &Configuration,
        configuration: &mut watch::Receiver<Configuration>,
        shutdown: &mut watch::Receiver<bool>,
        resume: &mut Resume,
        became_ready: &mut bool,
    ) -> Result<(), Exit> {
        let Resume {
            cursor,
            instance_id,
            connected_before,
        } = resume;
        let client = selected.client.as_ref().ok_or_else(|| {
            ClientError::InvalidConfiguration(
                "patinad client is not configured for this profile".into(),
            )
        })?;
        current(
            client.negotiate_tracking_owner(),
            configuration,
            selected.revision,
            shutdown,
        )
        .await?;
        // Legacy servers have no verifiable epoch: recover snapshots, not old alerts.
        if instance_id.is_none() {
            *cursor = None;
        }
        let mut events = current(
            client.open_runtime_event_stream(*cursor),
            configuration,
            selected.revision,
            shutdown,
        )
        .await?;
        let observed = events.instance_id().map(str::to_owned);
        if cursor.is_some() && observed != *instance_id {
            *cursor = None;
            drop(events);
            events = current(
                client.open_runtime_event_stream(None),
                configuration,
                selected.revision,
                shutdown,
            )
            .await?;
        }
        *instance_id = events.instance_id().map(str::to_owned);
        let mut snapshot = current(
            self.reader.read(client, *cursor),
            configuration,
            selected.revision,
            shutdown,
        )
        .await?;
        self.publish_snapshot(&snapshot, configuration, selected.revision, shutdown)?;
        publish(configuration, selected.revision, shutdown, || {
            if *connected_before {
                self.output
                    .resync_required("connection-reestablished", None);
            }
            self.output
                .connection_changed(ConnectionStatus::Ready, None);
        })?;
        *became_ready = true;
        *connected_before = true;
        let mut refresh = tokio::time::interval_at(
            tokio::time::Instant::now() + self.options.refresh_interval,
            self.options.refresh_interval,
        );
        refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            let next = current(
                async {
                    tokio::select! {
                        event = events.next_event() => event.map(Some),
                        _ = refresh.tick() => Ok(None),
                    }
                },
                configuration,
                selected.revision,
                shutdown,
            )
            .await?;
            let event = match next {
                None => {
                    snapshot = current(
                        self.reader.read(client, *cursor),
                        configuration,
                        selected.revision,
                        shutdown,
                    )
                    .await?;
                    self.publish_snapshot(&snapshot, configuration, selected.revision, shutdown)?;
                    continue;
                }
                Some(None) => {
                    return Err(
                        ClientError::Unreachable("patinad event stream closed".into()).into(),
                    )
                }
                Some(Some(event)) => event,
            };
            if event
                .sequence()
                .is_some_and(|sequence| cursor.is_some_and(|previous| sequence <= previous))
            {
                continue;
            }
            let next_cursor = event.sequence().or(*cursor);
            match event {
                StreamEvent::Runtime(envelope) => {
                    if self.reader.needs_refresh(&snapshot, &envelope) {
                        snapshot = current(
                            self.reader.read(client, next_cursor),
                            configuration,
                            selected.revision,
                            shutdown,
                        )
                        .await?;
                        self.publish_snapshot(
                            &snapshot,
                            configuration,
                            selected.revision,
                            shutdown,
                        )?;
                    }
                    publish(configuration, selected.revision, shutdown, || {
                        self.output.tracking_data_changed(&envelope)
                    })?;
                    *cursor = next_cursor;
                }
                StreamEvent::ResyncRequired { reason, missed } => {
                    *cursor = None;
                    snapshot = current(
                        self.reader.read(client, None),
                        configuration,
                        selected.revision,
                        shutdown,
                    )
                    .await?;
                    self.publish_snapshot(&snapshot, configuration, selected.revision, shutdown)?;
                    publish(configuration, selected.revision, shutdown, || {
                        self.output.resync_required(&reason, missed)
                    })?;
                }
                StreamEvent::Ignored { .. } => *cursor = next_cursor,
            }
        }
    }

    fn publish_snapshot(
        &self,
        snapshot: &R::Snapshot,
        configuration: &watch::Receiver<Configuration>,
        revision: u64,
        shutdown: &watch::Receiver<bool>,
    ) -> Result<(), Exit>
    where
        R::Snapshot: Clone,
    {
        publish(configuration, revision, shutdown, || {
            self.output.snapshot_changed(snapshot.clone())
        })
    }
}

fn stopped(shutdown: &watch::Receiver<bool>) -> bool {
    *shutdown.borrow() || shutdown.has_changed().is_err()
}

fn publish(
    configuration: &watch::Receiver<Configuration>,
    revision: u64,
    shutdown: &watch::Receiver<bool>,
    emit: impl FnOnce(),
) -> Result<(), Exit> {
    let stop = shutdown.borrow();
    let current = configuration.borrow();
    if *stop || shutdown.has_changed().is_err() {
        return Err(Exit::Shutdown);
    }
    if current.revision != revision {
        return Err(Exit::Reconfigure);
    }
    emit();
    Ok(())
}

async fn current<T>(
    future: impl Future<Output = Result<T, ClientError>>,
    configuration: &mut watch::Receiver<Configuration>,
    revision: u64,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<T, Exit> {
    if stopped(shutdown) {
        return Err(Exit::Shutdown);
    }
    if configuration.borrow().revision != revision {
        return Err(Exit::Reconfigure);
    }
    tokio::pin!(future);
    loop {
        tokio::select! {
            biased;
            result = shutdown.changed() => { if result.is_err() || *shutdown.borrow() { return Err(Exit::Shutdown); } }
            _ = configuration.changed() => return Err(Exit::Reconfigure),
            result = &mut future => return result.map_err(Exit::Failed),
        }
    }
}
