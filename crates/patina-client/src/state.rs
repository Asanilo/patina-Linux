use crate::{Client, ClientError};
use tokio::sync::watch;

/// Client and generation change atomically. A publication may hold a watch read
/// guard to prevent a configuration swap overtaking the final synchronous emit.
#[derive(Clone, Debug, Default)]
pub struct Configuration {
    pub revision: u64,
    pub client: Option<Client>,
}

#[derive(Clone, Debug)]
pub struct ClientState {
    sender: watch::Sender<Configuration>,
}

impl Default for ClientState {
    fn default() -> Self {
        let (sender, _) = watch::channel(Configuration::default());
        Self { sender }
    }
}

impl ClientState {
    pub fn install(&self, client: Client) {
        self.sender.send_modify(|configuration| {
            configuration.client = Some(client);
            configuration.revision = configuration.revision.wrapping_add(1);
        });
    }

    pub fn require(&self) -> Result<Client, ClientError> {
        self.sender.borrow().client.clone().ok_or_else(|| {
            ClientError::InvalidConfiguration(
                "patinad client is not configured for this profile".into(),
            )
        })
    }

    pub fn subscribe(&self) -> watch::Receiver<Configuration> {
        self.sender.subscribe()
    }
}
