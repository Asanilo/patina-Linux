//! Explicit-connection SDK probe. This is not the planned interactive TUI.
use patina_client::protocol::events::RuntimeEventEnvelope;
use patina_client::{
    state::ClientState,
    sync::{ConnectionStatus, SnapshotOutput, SnapshotReader, SnapshotSession},
    Client, ClientError, Negotiation,
};
use std::{error::Error, fs, path::Path, sync::Arc};

struct ServerReader;
impl SnapshotReader for ServerReader {
    type Snapshot = Negotiation;
    fn read<'a>(
        &'a self,
        client: &'a Client,
        _: Option<u64>,
    ) -> futures_util::future::BoxFuture<'a, Result<Negotiation, ClientError>> {
        Box::pin(client.negotiate_tracking_owner())
    }
    fn needs_refresh(&self, _: &Negotiation, _: &RuntimeEventEnvelope) -> bool {
        false
    }
}
struct MetadataOutput;
impl SnapshotOutput<Negotiation> for MetadataOutput {
    fn connection_changed(&self, status: ConnectionStatus, error: Option<&ClientError>) {
        if status == ConnectionStatus::Ready {
            println!("subscribed");
        } else {
            println!(
                "connection={status:?} error={}",
                error.map(ClientError::code).unwrap_or("none")
            );
        }
    }
    fn snapshot_changed(&self, _: Negotiation) {}
    fn tracking_data_changed(&self, event: &RuntimeEventEnvelope) {
        // Do not print titles, URLs, credentials or alert bodies.
        println!(
            "event={} cursor={}",
            event.event.event_name(),
            event.sequence
        );
    }
    fn resync_required(&self, _: &str, _: Option<u64>) {
        println!("resynchronized");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 || args.len() > 3 || (args.len() == 3 && args[2] != "--watch") {
        return Err("usage: inspect <loopback-port> <token-file> [--watch]".into());
    }
    let path = Path::new(&args[1]);
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > 4096 {
        return Err("expected a small regular credential file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("credential file must be owner-only".into());
        }
    }
    let token = fs::read_to_string(path)?;
    let client = Client::new(args[0].parse()?, token.trim())?;
    let negotiated = client.negotiate_tracking_owner().await?;
    println!(
        "server={} protocol={} tracking_ready={}",
        negotiated.server_version, negotiated.protocol_version, negotiated.tracking_ready
    );
    if args.len() == 3 {
        let state = ClientState::default();
        state.install(client);
        let session = SnapshotSession::new(state, ServerReader, Arc::new(MetadataOutput));
        let (stop, shutdown) = tokio::sync::watch::channel(false);
        let run = session.run(shutdown);
        tokio::pin!(run);
        tokio::select! {
            _ = &mut run => {},
            result = tokio::signal::ctrl_c() => { result?; stop.send(true)?; run.await; }
        }
    }
    Ok(())
}
