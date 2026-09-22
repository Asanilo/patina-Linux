//! Explicit private-bus fixture. No host login, sleep, service or environment changes.
use super::*;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};
use zbus::object_server::SignalContext;

#[derive(Default)]
struct State {
    selected: String,
    locked: bool,
}
struct Manager;
#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Manager {
    fn get_user(&self, _uid: u32) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/org/freedesktop/login1/user/test").unwrap()
    }
    #[zbus(signal)]
    async fn prepare_for_sleep(context: &SignalContext<'_>, start: bool) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn prepare_for_shutdown(context: &SignalContext<'_>, start: bool) -> zbus::Result<()>;
}
struct User(Arc<Mutex<State>>);
#[zbus::interface(name = "org.freedesktop.login1.User")]
impl User {
    #[zbus(property)]
    fn display(&self) -> (String, OwnedObjectPath) {
        let name = self.0.lock().unwrap().selected.clone();
        let path = if name.is_empty() {
            "/".into()
        } else {
            format!("/org/freedesktop/login1/session/{name}")
        };
        (name, OwnedObjectPath::try_from(path).unwrap())
    }
}
struct Session {
    name: String,
    state: Arc<Mutex<State>>,
}
#[zbus::interface(name = "org.freedesktop.login1.Session")]
impl Session {
    #[zbus(property)]
    fn id(&self) -> &str {
        &self.name
    }
    #[zbus(property, name = "Type")]
    fn kind(&self) -> &str {
        "wayland"
    }
    #[zbus(property)]
    fn class(&self) -> &str {
        "user"
    }
    #[zbus(property)]
    fn state(&self) -> &str {
        "active"
    }
    #[zbus(property)]
    fn remote(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn active(&self) -> bool {
        self.state.lock().unwrap().selected == self.name
    }
    #[zbus(property)]
    fn desktop(&self) -> &str {
        "GNOME"
    }
    #[zbus(property)]
    fn display(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn user(&self) -> (u32, OwnedObjectPath) {
        // SAFETY: geteuid is side-effect free and takes no pointers.
        (
            unsafe { libc::geteuid() },
            OwnedObjectPath::try_from("/org/freedesktop/login1/user/test").unwrap(),
        )
    }
    #[zbus(property)]
    fn locked_hint(&self) -> bool {
        self.state.lock().unwrap().locked
    }
    #[zbus(signal)]
    async fn lock(context: &SignalContext<'_>) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn unlock(context: &SignalContext<'_>) -> zbus::Result<()>;
}

async fn event(rx: &mut mpsc::Receiver<super::super::power::PowerLifecycleEvent>, expected: &str) {
    let event = tokio::time::timeout(Duration::from_secs(4), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.state, expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run inside dbus-run-session with PATINA_PRIVATE_LOGIND_TEST=1; never a host bus"]
async fn private_logind_late_login_logout_and_rebind_preserve_global_power_events() {
    assert_eq!(
        std::env::var("PATINA_PRIVATE_LOGIND_TEST").as_deref(),
        Ok("1")
    );
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
    assert!(
        address.starts_with("unix:path=/tmp/") || address.starts_with("unix:abstract=/tmp/"),
        "requires a private temporary bus"
    );
    let state = Arc::new(Mutex::new(State::default()));
    let server = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .name(SERVICE)
        .unwrap()
        .serve_at("/org/freedesktop/login1", Manager)
        .unwrap()
        .serve_at("/org/freedesktop/login1/user/test", User(state.clone()))
        .unwrap()
        .serve_at(
            "/org/freedesktop/login1/session/a",
            Session {
                name: "a".into(),
                state: state.clone(),
            },
        )
        .unwrap()
        .serve_at(
            "/org/freedesktop/login1/session/b",
            Session {
                name: "b".into(),
                state: state.clone(),
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let client = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    assert!(read_on(&client).await.is_err());
    let (stop, stopped) = watch::channel(false);
    let (tx, mut rx) = mpsc::channel(16);
    let connection = client.clone();
    let watcher = tokio::spawn(async move {
        super::super::power::watch_connection(&connection, stopped, tx).await
    });
    event(&mut rx, "ready").await;
    let manager = SignalContext::new(&server, "/org/freedesktop/login1").unwrap();
    Manager::prepare_for_sleep(&manager, true).await.unwrap();
    event(&mut rx, "suspend").await;
    Manager::prepare_for_sleep(&manager, false).await.unwrap();
    event(&mut rx, "resume").await;

    state.lock().unwrap().selected = "a".into();
    assert_eq!(
        read_on(&client).await.unwrap().context.session_type,
        "wayland"
    );
    event(&mut rx, "ready").await;
    event(&mut rx, "unlock").await;
    state.lock().unwrap().locked = true;
    let interface = server
        .object_server()
        .interface::<_, Session>("/org/freedesktop/login1/session/a")
        .await
        .unwrap();
    interface
        .get()
        .await
        .locked_hint_changed(interface.signal_context())
        .await
        .unwrap();
    event(&mut rx, "lock").await;
    state.lock().unwrap().selected.clear();
    assert!(read_on(&client).await.is_err());
    Manager::prepare_for_sleep(&manager, true).await.unwrap();
    event(&mut rx, "suspend").await;
    Manager::prepare_for_sleep(&manager, false).await.unwrap();
    event(&mut rx, "resume").await;

    {
        let mut state = state.lock().unwrap();
        state.selected = "b".into();
        state.locked = false;
    }
    // An old queued session signal must not affect the new login.
    Session::unlock(interface.signal_context()).await.unwrap();
    event(&mut rx, "ready").await;
    event(&mut rx, "unlock").await;
    assert!(read_on(&client)
        .await
        .unwrap()
        .path
        .as_str()
        .ends_with("/b"));
    Session::lock(interface.signal_context()).await.unwrap();
    let unexpected = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await;
    assert!(
        unexpected.is_err(),
        "unexpected old-session event: {unexpected:?}"
    );
    Manager::prepare_for_shutdown(&manager, true).await.unwrap();
    event(&mut rx, "shutdown").await;
    stop.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(2), watcher)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
