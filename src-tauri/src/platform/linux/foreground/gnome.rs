//! Read either GNOME companion protocol without treating transport failures as
//! an empty desktop. The packaged legacy extension remains unchanged.
use super::{ForegroundProbeError, WindowInfo};
use zbus::blocking::{Connection, Proxy};

const SNAPSHOT_BUS: &str = "org.patina.WindowTracker1";
const SNAPSHOT_PATH: &str = "/org/patina/WindowTracker1";
const LEGACY_BUS: &str = "org.patina.WindowTracker";
const LEGACY_PATH: &str = "/org/patina/WindowTracker";
const MAX_TITLE_BYTES: usize = 4096;
const MAX_IDENTITY_BYTES: usize = 512;
const MAX_WINDOW_ID_BYTES: usize = 64;
const MAX_REPLY_BODY_BYTES: usize = 8192;

type SnapshotReply = (u32, u32, String, String, String, u32, String);
type LegacyReply = (String, String, String, u32, u64);

struct WindowFacts {
    title: String,
    app_id: String,
    wm_class: String,
    pid: u32,
    window_id: String,
}

trait Transport {
    fn owner(&mut self, name: &str) -> Result<Option<String>, ForegroundProbeError>;
    fn snapshot(&mut self, owner: &str) -> Result<SnapshotReply, ForegroundProbeError>;
    fn legacy(&mut self, owner: &str) -> Result<LegacyReply, ForegroundProbeError>;
}

struct SessionTransport(Connection);

impl Transport for SessionTransport {
    fn owner(&mut self, name: &str) -> Result<Option<String>, ForegroundProbeError> {
        let bus = Proxy::new(
            &self.0,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )
        .map_err(|_| ForegroundProbeError::WindowUnavailable)?;
        match bus.call_method("GetNameOwner", &(name,)) {
            Ok(reply) => {
                let body = reply.body();
                validate_body(&body, "s")?;
                let owner: String = body
                    .deserialize()
                    .map_err(|_| ForegroundProbeError::WindowUnavailable)?;
                zbus::names::UniqueName::try_from(owner.as_str())
                    .map_err(|_| ForegroundProbeError::WindowUnavailable)?;
                Ok(Some(owner))
            }
            Err(error) if is_missing_owner(&error) => Ok(None),
            Err(_) => Err(ForegroundProbeError::WindowUnavailable),
        }
    }

    fn snapshot(&mut self, owner: &str) -> Result<SnapshotReply, ForegroundProbeError> {
        // Pin the unique owner. An extension disappearing or being replaced
        // during this sample is a failure, not permission to change protocols.
        let proxy = Proxy::new(&self.0, owner, SNAPSHOT_PATH, SNAPSHOT_BUS)
            .map_err(|_| ForegroundProbeError::WindowUnavailable)?;
        let reply = proxy.call_method("GetSnapshot", &()).map_err(rpc_error)?;
        let body = reply.body();
        validate_body(&body, "uusssus")?;
        body.deserialize()
            .map_err(|_| ForegroundProbeError::InvalidWindowResponse)
    }

    fn legacy(&mut self, owner: &str) -> Result<LegacyReply, ForegroundProbeError> {
        let proxy = Proxy::new(&self.0, owner, LEGACY_PATH, LEGACY_BUS)
            .map_err(|_| ForegroundProbeError::WindowUnavailable)?;
        let reply = proxy
            .call_method("GetFocusedWindow", &())
            .map_err(rpc_error)?;
        let body = reply.body();
        validate_body(&body, "sssut")?;
        body.deserialize()
            .map_err(|_| ForegroundProbeError::InvalidWindowResponse)
    }
}

fn is_missing_owner(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::MethodError(name, _, _) => {
            name.as_str() == "org.freedesktop.DBus.Error.NameHasNoOwner"
        }
        zbus::Error::FDO(error) => matches!(error.as_ref(), zbus::fdo::Error::NameHasNoOwner(_)),
        _ => false,
    }
}

fn rpc_error(error: zbus::Error) -> ForegroundProbeError {
    match error {
        zbus::Error::Variant(_) | zbus::Error::InvalidReply | zbus::Error::InvalidField => {
            ForegroundProbeError::InvalidWindowResponse
        }
        zbus::Error::MethodError(name, _, _)
            if matches!(
                name.as_str(),
                "org.freedesktop.DBus.Error.UnknownMethod"
                    | "org.freedesktop.DBus.Error.UnknownInterface"
                    | "org.freedesktop.DBus.Error.InvalidArgs"
            ) =>
        {
            ForegroundProbeError::InvalidWindowResponse
        }
        _ => ForegroundProbeError::WindowUnavailable,
    }
}

fn validate_body(body: &zbus::message::Body, signature: &str) -> Result<(), ForegroundProbeError> {
    // Check the received body before allocating owned strings during decoding.
    if body.len() > MAX_REPLY_BODY_BYTES
        || body.signature().as_ref().map(|value| value.as_str()) != Some(signature)
    {
        return Err(ForegroundProbeError::InvalidWindowResponse);
    }
    Ok(())
}

pub(super) fn query_focused_window() -> Result<Option<WindowInfo>, ForegroundProbeError> {
    let connection = Connection::session().map_err(|_| ForegroundProbeError::WindowUnavailable)?;
    query_with(
        &mut SessionTransport(connection),
        super::get_process_details,
    )
}

fn query_with(
    transport: &mut impl Transport,
    mut process_details: impl FnMut(u32) -> (String, String),
) -> Result<Option<WindowInfo>, ForegroundProbeError> {
    let facts = if let Some(owner) = transport.owner(SNAPSHOT_BUS)? {
        decode_snapshot(transport.snapshot(&owner)?)?
    } else if let Some(owner) = transport.owner(LEGACY_BUS)? {
        decode_legacy(transport.legacy(&owner)?)?
    } else {
        return Err(ForegroundProbeError::ProviderUnavailable);
    };
    let Some(facts) = facts else {
        return Ok(None);
    };
    // Validate all wire fields before touching /proc or the metadata cache.
    let (exe_name, process_path) = if facts.pid > 0 {
        process_details(facts.pid)
    } else {
        (String::new(), String::new())
    };
    let exe_name = if exe_name.is_empty() {
        facts.app_id
    } else {
        exe_name
    };
    if exe_name.trim().is_empty() {
        return Err(ForegroundProbeError::WindowUnavailable);
    }
    Ok(Some(WindowInfo {
        hwnd: facts.window_id.clone(),
        root_owner_hwnd: facts.window_id,
        process_id: facts.pid,
        window_class: facts.wm_class,
        title: facts.title,
        exe_name,
        process_path,
        // The parent provider attaches independently validated idle data.
        is_afk: false,
        idle_time_ms: 0,
    }))
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    value.len() <= max_bytes && !value.contains('\0')
}

fn validate_text_fields(
    title: &str,
    app_id: &str,
    wm_class: &str,
) -> Result<(), ForegroundProbeError> {
    if !valid_text(title, MAX_TITLE_BYTES)
        || !valid_text(app_id, MAX_IDENTITY_BYTES)
        || !valid_text(wm_class, MAX_IDENTITY_BYTES)
    {
        return Err(ForegroundProbeError::InvalidWindowResponse);
    }
    Ok(())
}

fn decode_snapshot(reply: SnapshotReply) -> Result<Option<WindowFacts>, ForegroundProbeError> {
    let (version, state, title, desktop_id, wm_class, pid, window_id) = reply;
    if version != 1 || !valid_text(&window_id, MAX_WINDOW_ID_BYTES) {
        return Err(ForegroundProbeError::InvalidWindowResponse);
    }
    validate_text_fields(&title, &desktop_id, &wm_class)?;
    let empty = title.is_empty()
        && desktop_id.is_empty()
        && wm_class.is_empty()
        && pid == 0
        && window_id.is_empty();
    match state {
        // No-window includes overview. Locked is also a known empty observation;
        // it does not synthesize a persistent logind lifecycle event.
        0 | 2 if empty => Ok(None),
        3 if empty => Err(ForegroundProbeError::WindowUnavailable),
        1 if !window_id.trim().is_empty() && (pid != 0 || !desktop_id.trim().is_empty()) => {
            Ok(Some(WindowFacts {
                title,
                // Keep the fork's application key when /proc is unavailable.
                app_id: desktop_id
                    .strip_suffix(".desktop")
                    .unwrap_or(&desktop_id)
                    .to_owned(),
                wm_class,
                pid,
                window_id,
            }))
        }
        _ => Err(ForegroundProbeError::InvalidWindowResponse),
    }
}

fn decode_legacy(reply: LegacyReply) -> Result<Option<WindowFacts>, ForegroundProbeError> {
    let (title, app_id, wm_class, pid, window_id) = reply;
    validate_text_fields(&title, &app_id, &wm_class)?;
    if title.is_empty() && app_id.is_empty() && wm_class.is_empty() && pid == 0 && window_id == 0 {
        return Ok(None);
    }
    if pid == 0 && app_id.trim().is_empty() {
        return Err(ForegroundProbeError::InvalidWindowResponse);
    }
    Ok(Some(WindowFacts {
        title,
        app_id,
        wm_class,
        pid,
        window_id: window_id.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    enum Step {
        Owner(
            &'static str,
            Result<Option<&'static str>, ForegroundProbeError>,
        ),
        Snapshot(Result<SnapshotReply, ForegroundProbeError>),
        Legacy(Result<LegacyReply, ForegroundProbeError>),
    }

    struct FakeTransport(VecDeque<Step>);

    impl Transport for FakeTransport {
        fn owner(&mut self, name: &str) -> Result<Option<String>, ForegroundProbeError> {
            let Some(Step::Owner(expected, result)) = self.0.pop_front() else {
                panic!("unexpected owner lookup")
            };
            assert_eq!(name, expected);
            result.map(|owner| owner.map(str::to_owned))
        }
        fn snapshot(&mut self, owner: &str) -> Result<SnapshotReply, ForegroundProbeError> {
            assert_eq!(owner, ":1.10");
            let Some(Step::Snapshot(result)) = self.0.pop_front() else {
                panic!("unexpected versioned call")
            };
            result
        }
        fn legacy(&mut self, owner: &str) -> Result<LegacyReply, ForegroundProbeError> {
            assert_eq!(owner, ":1.20");
            let Some(Step::Legacy(result)) = self.0.pop_front() else {
                panic!("unexpected legacy call")
            };
            result
        }
    }

    fn empty(state: u32) -> SnapshotReply {
        (
            1,
            state,
            String::new(),
            String::new(),
            String::new(),
            0,
            String::new(),
        )
    }

    fn focused() -> SnapshotReply {
        (
            1,
            1,
            "Synthetic title".into(),
            "org.example.App.desktop".into(),
            "ExampleClass".into(),
            42,
            "17".into(),
        )
    }

    fn versioned(result: Result<SnapshotReply, ForegroundProbeError>) -> FakeTransport {
        FakeTransport(VecDeque::from([
            Step::Owner(SNAPSHOT_BUS, Ok(Some(":1.10"))),
            Step::Snapshot(result),
        ]))
    }

    #[test]
    fn new_protocol_is_preferred_and_keeps_existing_process_identity() {
        let mut transport = versioned(Ok(focused()));
        let window = query_with(&mut transport, |pid| {
            assert_eq!(pid, 42);
            ("example".into(), "/synthetic/example".into())
        })
        .unwrap()
        .unwrap();
        assert_eq!(window.exe_name, "example");
        assert_eq!(window.process_path, "/synthetic/example");
        assert_eq!(window.window_class, "ExampleClass");
        assert_eq!(window.hwnd, "17");
        assert!(transport.0.is_empty());
    }

    #[test]
    fn only_missing_new_owner_allows_legacy_and_retains_u64_window_ids() {
        let mut transport = FakeTransport(VecDeque::from([
            Step::Owner(SNAPSHOT_BUS, Ok(None)),
            Step::Owner(LEGACY_BUS, Ok(Some(":1.20"))),
            Step::Legacy(Ok((
                "Synthetic".into(),
                "example".into(),
                "Class".into(),
                0,
                u64::MAX,
            ))),
        ]));
        let window = query_with(&mut transport, |_| panic!("zero PID must not read /proc"))
            .unwrap()
            .unwrap();
        assert_eq!(window.exe_name, "example");
        assert_eq!(window.hwnd, u64::MAX.to_string());
        assert!(transport.0.is_empty());
    }

    #[test]
    fn missing_providers_are_not_an_empty_desktop() {
        let mut transport = FakeTransport(VecDeque::from([
            Step::Owner(SNAPSHOT_BUS, Ok(None)),
            Step::Owner(LEGACY_BUS, Ok(None)),
        ]));
        assert!(matches!(
            query_with(&mut transport, |_| unreachable!()),
            Err(ForegroundProbeError::ProviderUnavailable)
        ));
        let mut transport = FakeTransport(VecDeque::from([Step::Owner(
            SNAPSHOT_BUS,
            Err(ForegroundProbeError::WindowUnavailable),
        )]));
        assert!(matches!(
            query_with(&mut transport, |_| unreachable!()),
            Err(ForegroundProbeError::WindowUnavailable)
        ));
    }

    #[test]
    fn new_protocol_failures_never_try_legacy_or_process_metadata() {
        for error in [
            ForegroundProbeError::WindowUnavailable,
            ForegroundProbeError::InvalidWindowResponse,
        ] {
            let mut transport = versioned(Err(error));
            assert!(query_with(&mut transport, |_| panic!("invalid data reached /proc")).is_err());
            assert!(transport.0.is_empty());
        }
        for reply in [
            empty(3),
            empty(99),
            (
                2,
                0,
                String::new(),
                String::new(),
                String::new(),
                0,
                String::new(),
            ),
        ] {
            assert!(query_with(&mut versioned(Ok(reply)), |_| unreachable!()).is_err());
        }
    }

    #[test]
    fn only_empty_no_window_and_locked_states_are_successful_absence() {
        for state in [0, 2] {
            assert!(
                query_with(&mut versioned(Ok(empty(state))), |_| unreachable!())
                    .unwrap()
                    .is_none()
            );
            let mut invalid = empty(state);
            invalid.2 = "previous private title".into();
            assert!(matches!(
                decode_snapshot(invalid),
                Err(ForegroundProbeError::InvalidWindowResponse)
            ));
        }
        assert!(
            decode_legacy((String::new(), String::new(), String::new(), 0, 0))
                .unwrap()
                .is_none()
        );
        assert!(decode_legacy(("title only".into(), String::new(), String::new(), 0, 9)).is_err());
    }

    #[test]
    fn fields_are_byte_bounded_and_invalid_identity_is_rejected_before_projection() {
        let mut cases = Vec::new();
        let mut value = focused();
        value.2 = "界".repeat(1366);
        cases.push(value);
        let mut value = focused();
        value.3 = "a".repeat(513);
        cases.push(value);
        let mut value = focused();
        value.4 = "a\0b".into();
        cases.push(value);
        let mut value = focused();
        value.6 = "a".repeat(65);
        cases.push(value);
        let mut value = focused();
        value.6 = " ".into();
        cases.push(value);
        let mut value = focused();
        value.3 = " ".into();
        value.5 = 0;
        cases.push(value);
        for reply in cases {
            assert!(matches!(
                query_with(&mut versioned(Ok(reply)), |_| panic!(
                    "invalid data reached /proc"
                )),
                Err(ForegroundProbeError::InvalidWindowResponse)
            ));
        }
        assert!(decode_legacy(("a".repeat(4097), "app".into(), String::new(), 1, 1)).is_err());
        let mut reply = focused();
        reply.2 = "界".repeat(1365);
        reply.5 = 0;
        let window = query_with(&mut versioned(Ok(reply)), |_| unreachable!())
            .unwrap()
            .unwrap();
        assert_eq!(window.exe_name, "org.example.App");
        assert_eq!(window.title.len(), 4095);
    }

    #[test]
    fn unresolved_window_identity_is_not_reported_as_a_successful_empty_desktop() {
        let mut reply = focused();
        reply.3.clear();
        assert!(matches!(
            query_with(&mut versioned(Ok(reply)), |_| (
                String::new(),
                String::new()
            )),
            Err(ForegroundProbeError::WindowUnavailable)
        ));
        let mut reply = focused();
        reply.3 = ".desktop".into();
        reply.5 = 0;
        assert!(query_with(&mut versioned(Ok(reply)), |_| unreachable!()).is_err());
    }

    #[test]
    fn typed_wire_shape_and_body_size_are_checked_before_string_decoding() {
        let valid = zbus::Message::method("/synthetic", "Snapshot")
            .unwrap()
            .build(&focused())
            .unwrap();
        assert!(validate_body(&valid.body(), "uusssus").is_ok());
        assert!(validate_body(&valid.body(), "sssut").is_err());
        let oversized = zbus::Message::method("/synthetic", "Snapshot")
            .unwrap()
            .build(&("a".repeat(MAX_REPLY_BODY_BYTES),))
            .unwrap();
        assert!(validate_body(&oversized.body(), "s").is_err());
        assert!(is_missing_owner(&zbus::Error::FDO(Box::new(
            zbus::fdo::Error::NameHasNoOwner("synthetic".into())
        ))));
        assert!(!is_missing_owner(&zbus::Error::FDO(Box::new(
            zbus::fdo::Error::AccessDenied("synthetic".into())
        ))));
    }
}
