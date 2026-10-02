use patina_client::{
    events::{parse_stream_event, StreamEvent},
    Event,
};

fn frame(name: &str, id: &str, data: &str) -> Event {
    Event {
        event: name.into(),
        id: id.into(),
        data: data.into(),
        retry: None,
    }
}

#[test]
fn typed_events_require_matching_name_id_and_envelope() {
    let data = r#"{"sequence":7,"event":{"type":"tracking-data-changed","reason":"classification-changed","changed_at_ms":1}}"#;
    assert!(matches!(
        parse_stream_event(frame("tracking-data-changed", "7", data)),
        Ok(StreamEvent::Runtime(_))
    ));
    for (name, id) in [
        ("tracking-data-changed", "8"),
        ("tools-runtime-changed", "7"),
        ("tracking-data-changed", ""),
        ("tracking-data-changed", "not-a-cursor"),
    ] {
        assert_eq!(
            parse_stream_event(frame(name, id, data))
                .unwrap_err()
                .code(),
            "invalid-response"
        );
    }
}

#[test]
fn future_events_preserve_cursor_without_becoming_known_domain_events() {
    assert_eq!(
        parse_stream_event(frame("future-change", "9", "{}")).unwrap(),
        StreamEvent::Ignored {
            event: "future-change".into(),
            sequence: Some(9)
        }
    );
    assert!(parse_stream_event(frame("future-change", "-1", "{}")).is_err());
}

#[test]
fn resync_is_explicit_and_oversized_data_is_rejected() {
    assert_eq!(
        parse_stream_event(frame(
            "resync-required",
            "",
            r#"{"reason":"replay-gap","missed":3}"#
        ))
        .unwrap(),
        StreamEvent::ResyncRequired {
            reason: "replay-gap".into(),
            missed: Some(3)
        }
    );
    assert!(parse_stream_event(frame("resync-required", "", "{}")).is_err());
    assert_eq!(
        parse_stream_event(frame("future-event", "1", &"x".repeat(65_537)))
            .unwrap_err()
            .code(),
        "response-too-large"
    );
}
