//! Current graphical-session facts. A user service can outlive desktop sessions;
//! its inherited environment is not authoritative and must never be mutated.
use std::collections::HashMap;
use std::time::Duration;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const SERVICE: &str = "org.freedesktop.login1";
const DEADLINE: Duration = Duration::from_millis(700);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Context {
    pub session_type: String,
    pub desktop: Option<String>,
    pub display: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GraphicalSession {
    pub path: OwnedObjectPath,
    pub context: Context,
}

// Used from the existing foreground/diagnostics blocking workers. The async
// operation also has a deadline, so a timed-out caller leaves no unbounded task.
pub(super) fn current() -> Result<Context, &'static str> {
    let managed = std::env::var_os("PATINA_SYSTEMD_SERVICE").is_some();
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    if !managed {
        if let Some(kind) = session_type.filter(|value| !value.trim().is_empty()) {
            return environment_context(
                &kind,
                std::env::var("XDG_CURRENT_DESKTOP")
                    .or_else(|_| std::env::var("DESKTOP_SESSION"))
                    .ok(),
                std::env::var("DISPLAY").ok(),
            );
        }
    }
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    tauri::async_runtime::spawn(async move {
        let result = tokio::time::timeout(DEADLINE, async {
            let connection = zbus::Connection::system()
                .await
                .map_err(|_| "logind-unavailable")?;
            read_on(&connection).await.map(|session| session.context)
        })
        .await
        .unwrap_or(Err("session-query-timeout"));
        let _ = tx.send(result);
    });
    rx.recv_timeout(DEADLINE + Duration::from_millis(100))
        .unwrap_or(Err("session-query-timeout"))
}

fn environment_context(
    kind: &str,
    desktop: Option<String>,
    display: Option<String>,
) -> Result<Context, &'static str> {
    let kind = kind.trim().to_ascii_lowercase();
    if !matches!(kind.as_str(), "wayland" | "x11") {
        return Err("session-not-graphical");
    }
    Ok(Context {
        session_type: kind,
        desktop,
        display,
    })
}

pub(super) async fn read_on(
    connection: &zbus::Connection,
) -> Result<GraphicalSession, &'static str> {
    // SAFETY: geteuid has no pointer arguments or side effects.
    let uid = unsafe { libc::geteuid() };
    tokio::time::timeout(DEADLINE, read_user(connection, uid))
        .await
        .unwrap_or(Err("session-query-timeout"))
}

async fn read_user(
    connection: &zbus::Connection,
    uid: u32,
) -> Result<GraphicalSession, &'static str> {
    let reply = connection
        .call_method(
            Some(SERVICE),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "GetUser",
            &(uid,),
        )
        .await
        .map_err(|_| "session-user-unavailable")?;
    let user: OwnedObjectPath = reply.body().deserialize().map_err(|_| "session-invalid")?;
    let display = read_display(connection, &user).await?;
    if display.0.is_empty() || display.1.as_str() == "/" {
        return Err("session-display-unavailable");
    }
    let reply = connection
        .call_method(
            Some(SERVICE),
            display.1.as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "GetAll",
            &("org.freedesktop.login1.Session",),
        )
        .await
        .map_err(|_| "session-properties-unavailable")?;
    let properties: HashMap<String, OwnedValue> =
        reply.body().deserialize().map_err(|_| "session-invalid")?;
    let context = validate(&properties, uid, &display.0)?;
    // Do not combine properties from one login with a later Display selection.
    if read_display(connection, &user).await? != display {
        return Err("session-changed-during-query");
    }
    Ok(GraphicalSession {
        path: display.1,
        context,
    })
}

async fn read_display(
    connection: &zbus::Connection,
    user: &OwnedObjectPath,
) -> Result<(String, OwnedObjectPath), &'static str> {
    let reply = connection
        .call_method(
            Some(SERVICE),
            user.as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.freedesktop.login1.User", "Display"),
        )
        .await
        .map_err(|_| "session-display-unavailable")?;
    let value: OwnedValue = reply.body().deserialize().map_err(|_| "session-invalid")?;
    value.try_into().map_err(|_| "session-invalid")
}

fn validate(
    properties: &HashMap<String, OwnedValue>,
    uid: u32,
    id: &str,
) -> Result<Context, &'static str> {
    let text = |name: &str| {
        properties
            .get(name)
            .and_then(|value| <&str>::try_from(value).ok())
            .ok_or("session-invalid")
    };
    let flag = |name: &str| {
        properties
            .get(name)
            .and_then(|value| bool::try_from(value).ok())
            .ok_or("session-invalid")
    };
    let user: (u32, OwnedObjectPath) = properties
        .get("User")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| value.try_into().ok())
        .ok_or("session-invalid")?;
    if user.0 != uid
        || text("Id")? != id
        || text("Class")? != "user"
        || flag("Remote")?
        || !flag("Active")?
        || text("State")? != "active"
    {
        return Err("session-not-active-local-user");
    }
    let desktop = text("Desktop")?;
    let display = text("Display")?;
    if desktop.len() > 512
        || display.len() > 256
        || desktop.contains('\0')
        || display.contains('\0')
    {
        return Err("session-invalid");
    }
    if text("Type")? == "x11" && display.is_empty() {
        return Err("session-display-unavailable");
    }
    environment_context(
        text("Type")?,
        (!desktop.is_empty()).then(|| desktop.to_string()),
        (!display.is_empty()).then(|| display.to_string()),
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests;
