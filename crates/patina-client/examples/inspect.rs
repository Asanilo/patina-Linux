//! Explicit-connection SDK probe. This is not the planned interactive TUI.
use patina_client::Client;
use std::{error::Error, fs, path::Path};

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
        let mut events = client.open_event_stream(None).await?;
        println!("subscribed");
        while let Some(event) = events.next_event().await? {
            // Do not print titles, URLs, credentials or alert bodies.
            println!("event={} cursor={}", event.event, event.id);
        }
    }
    Ok(())
}
