use patina_protocol::activity::ActivityReadHealth;
use sqlx::SqliteConnection;

pub async fn read_health(
    connection: &mut SqliteConnection,
    sampled_at_ms: i64,
) -> Result<ActivityReadHealth, String> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT CASE WHEN length(CAST(value AS BLOB)) <= 32 THEN value ELSE '' END FROM settings WHERE key = ?")
            .bind(super::tracker_settings::TRACKER_LAST_HEARTBEAT_KEY)
            .fetch_optional(connection)
            .await
            .map_err(|e| format!("activity heartbeat read failed: {e}"))?;
    let heartbeat = raw.and_then(|value| value.parse::<i64>().ok());
    Ok(crate::domain::activity_read_health::resolve_read_health(
        heartbeat,
        sampled_at_ms,
    ))
}
