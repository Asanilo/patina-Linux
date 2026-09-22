use super::ForegroundProbeError;

pub(super) fn query_idle_time_ms(
    session_type: Option<&str>,
    display: Option<&str>,
) -> Result<u32, ForegroundProbeError> {
    query_with(session_type, query_mutter, || query_x11(display))
}

fn query_with(
    session_type: Option<&str>,
    mutter: impl FnOnce() -> Option<u32>,
    x11: impl FnOnce() -> Option<u32>,
) -> Result<u32, ForegroundProbeError> {
    let value = match session_type.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("wayland") => mutter(),
        Some(value) if value.eq_ignore_ascii_case("x11") => mutter().or_else(x11),
        _ => return Err(ForegroundProbeError::UnsupportedSession),
    };
    // Zero is a valid observation. Missing observations must never become zero,
    // and XWayland cannot prove the idle state of a Wayland session.
    value.ok_or(ForegroundProbeError::IdleUnavailable)
}

fn query_mutter() -> Option<u32> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.gnome.Mutter.IdleMonitor",
        "/org/gnome/Mutter/IdleMonitor/Core",
        "org.gnome.Mutter.IdleMonitor",
    )
    .ok()?;
    let value: u64 = proxy
        .call_method("GetIdletime", &())
        .ok()?
        .body()
        .deserialize()
        .ok()?;
    Some(mutter_idle_time_to_ms(value))
}

fn mutter_idle_time_to_ms(value: u64) -> u32 {
    value.min(u64::from(u32::MAX)) as u32
}

fn query_x11(display: Option<&str>) -> Option<u32> {
    let (conn, screen_num) = xcb::Connection::connect(display).ok()?;
    let setup = conn.get_setup();
    let root = setup.roots().nth(screen_num as usize)?.root();
    let cookie = conn.send_request(&xcb::screensaver::QueryInfo {
        drawable: xcb::x::Drawable::Window(root),
    });
    conn.wait_for_reply(cookie)
        .ok()
        .map(|reply| reply.ms_since_user_input())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_mutter_failure_is_unknown_without_querying_xwayland() {
        let result = query_with(Some(" Wayland "), || None, || panic!("must not query X11"));
        assert_eq!(result, Err(ForegroundProbeError::IdleUnavailable));
        assert_eq!(result.unwrap_err().to_string(), "linux-idle-unavailable");
    }

    #[test]
    fn known_zero_is_preserved_and_does_not_trigger_fallback() {
        for session in ["wayland", "x11"] {
            assert_eq!(
                query_with(Some(session), || Some(0), || panic!("unneeded X11 query")),
                Ok(0)
            );
        }
    }

    #[test]
    fn x11_falls_back_only_when_mutter_is_unavailable() {
        assert_eq!(query_with(Some("X11"), || None, || Some(234)), Ok(234));
        assert_eq!(query_with(Some("x11"), || None, || Some(0)), Ok(0));
        assert_eq!(
            query_with(Some("x11"), || None, || None),
            Err(ForegroundProbeError::IdleUnavailable)
        );
    }

    #[test]
    fn unknown_session_does_not_guess_an_idle_provider() {
        for session in [None, Some(""), Some("tty")] {
            assert_eq!(
                query_with(
                    session,
                    || panic!("unexpected Mutter"),
                    || panic!("unexpected X11")
                ),
                Err(ForegroundProbeError::UnsupportedSession)
            );
        }
    }

    #[test]
    fn mutter_values_are_milliseconds_and_saturate_without_wrapping() {
        assert_eq!(mutter_idle_time_to_ms(181), 181);
        assert_eq!(mutter_idle_time_to_ms(u64::MAX), u32::MAX);
    }
}
