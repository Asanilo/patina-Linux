pub fn send(title: &str, body: &str) -> Result<(), String> {
    notify_rust::Notification::new()
        .appname("Patina")
        .summary(title)
        .body(body)
        .icon("patina")
        .show()
        .map(|_| ())
        .map_err(|error| format!("failed to send Linux desktop notification: {error}"))
}
