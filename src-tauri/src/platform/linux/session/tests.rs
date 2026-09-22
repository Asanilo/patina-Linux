use super::*;
use zbus::zvariant::Value;

fn text(value: &str) -> OwnedValue {
    Value::from(value).try_into().unwrap()
}

fn facts() -> HashMap<String, OwnedValue> {
    HashMap::from([
        ("Id".into(), text("desktop-a")),
        ("Type".into(), text("wayland")),
        ("Class".into(), text("user")),
        ("State".into(), text("active")),
        ("Remote".into(), OwnedValue::from(false)),
        ("Active".into(), OwnedValue::from(true)),
        ("Desktop".into(), text("GNOME")),
        ("Display".into(), text("")),
        (
            "User".into(),
            Value::from((1000u32, OwnedObjectPath::try_from("/user/test").unwrap()))
                .try_into()
                .unwrap(),
        ),
    ])
}

#[test]
fn validates_current_local_graphical_session_without_environment() {
    let result = validate(&facts(), 1000, "desktop-a").unwrap();
    assert_eq!(result.session_type, "wayland");
    assert_eq!(result.desktop.as_deref(), Some("GNOME"));
    assert_eq!(result.display, None);
    let mut x11 = facts();
    x11.insert("Type".into(), text("x11"));
    x11.insert("Display".into(), text(":4"));
    assert_eq!(
        validate(&x11, 1000, "desktop-a")
            .unwrap()
            .display
            .as_deref(),
        Some(":4")
    );
}

#[test]
fn rejects_other_users_remote_inactive_closing_greeter_and_changed_session() {
    assert!(validate(&facts(), 999, "desktop-a").is_err());
    assert!(validate(&facts(), 1000, "desktop-b").is_err());
    for (key, value) in [
        ("Remote", OwnedValue::from(true)),
        ("Active", OwnedValue::from(false)),
        ("State", text("closing")),
        ("Class", text("greeter")),
        ("Type", text("tty")),
        ("Desktop", text(&"x".repeat(513))),
    ] {
        let mut input = facts();
        input.insert(key.into(), value);
        assert!(validate(&input, 1000, "desktop-a").is_err(), "{key}");
    }
}

#[test]
fn missing_or_malformed_properties_are_not_guessed() {
    for key in facts().keys() {
        let mut input = facts();
        input.remove(key);
        assert!(validate(&input, 1000, "desktop-a").is_err(), "{key}");
        input.insert(key.clone(), OwnedValue::from(77u32));
        assert!(validate(&input, 1000, "desktop-a").is_err(), "{key}");
    }
}

#[test]
fn explicit_non_graphical_environment_stays_unsupported() {
    for kind in ["tty", "mir", ""] {
        assert!(environment_context(kind, Some("GNOME".into()), None).is_err());
    }
    assert_eq!(
        environment_context(" Wayland ", None, None)
            .unwrap()
            .session_type,
        "wayland"
    );
}

#[tokio::test]
#[ignore = "read-only host logind capability check; never reads focus or changes a service"]
async fn host_logind_resolves_graphical_session_without_environment() {
    let connection = zbus::Connection::system().await.unwrap();
    let result = read_on(&connection).await.unwrap();
    assert_eq!(
        std::env::var("PATINA_SYSTEMD_SERVICE").as_deref(),
        Ok("patinad.service")
    );
    assert!(std::env::var_os("XDG_SESSION_TYPE").is_none());
    assert!(std::env::var_os("XDG_CURRENT_DESKTOP").is_none());
    let observed = tokio::task::spawn_blocking(current).await.unwrap().unwrap();
    assert_eq!(observed, result.context);
    let diagnostic =
        tokio::task::spawn_blocking(super::super::foreground::window_tracking_diagnostics)
            .await
            .unwrap();
    assert_eq!(
        diagnostic.session_type.as_deref(),
        Some(result.context.session_type.as_str())
    );
    println!(
        "diagnostic_status={}, provider={}",
        diagnostic.status, diagnostic.provider
    );
    assert!(matches!(
        result.context.session_type.as_str(),
        "wayland" | "x11"
    ));
    println!(
        "session_type={}, desktop={:?}",
        result.context.session_type, result.context.desktop
    );
}
