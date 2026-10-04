use super::*;
use patina_protocol::activity::{ActivityReadHealth, ActivityReadStatus};

fn snapshot(start: i64, end: i64) -> ExactHistorySnapshot {
    ExactHistorySnapshot {
        from_ms: start,
        to_ms: end,
        sampled_at_ms: end,
        configuration_revision: "a".repeat(64),
        tracking_health: ActivityReadHealth {
            status: ActivityReadStatus::Unavailable,
            last_heartbeat_ms: None,
            live_cutoff_ms: 0,
            stale_after_ms: 8000,
        },
        records: vec![ExactActivityRecord {
            origin: ExactActivityOrigin::Native,
            record_id: 1,
            app_key: "fixture".into(),
            app_name: "Fixture".into(),
            exe_name: "fixture".into(),
            category: "development".into(),
            display_name_override: None,
            window_title: String::new(),
            start_ms: start,
            end_ms: end,
            continuity_start_ms: start,
            is_open: false,
            title_samples: vec![],
        }],
    }
}

#[test]
fn host_local_hourly_projection_matches_real_minutes_across_clock_changes() {
    use chrono::Datelike;
    for (month, day) in [(3, 8), (11, 1), (4, 5), (10, 4), (3, 29), (10, 25), (9, 27)] {
        let date = chrono::NaiveDate::from_ymd_opt(2026, month, day).unwrap();
        let start = chrono::Local
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .unwrap()
            .timestamp_millis();
        let end = chrono::Local
            .from_local_datetime(&date.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .unwrap()
            .timestamp_millis();
        let hours = project_hours(&chrono::Local, &snapshot(start, end)).unwrap();
        let mut expected = [0i64; 24];
        for minute in (start..end).step_by(60_000) {
            let time = chrono::Local.timestamp_millis_opt(minute).unwrap();
            assert_eq!(time.day(), date.day());
            expected[time.hour() as usize] += (end - minute).min(60_000);
        }
        assert_eq!(
            hours.iter().map(|hour| hour.active_ms).collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            hours.iter().map(|hour| hour.active_ms).sum::<i64>(),
            end - start
        );
        if std::env::var("TZ").as_deref() == Ok("Antarctica/Troll") && (month, day) == (10, 25) {
            assert_eq!((end - start) / 3_600_000, 26);
            assert_eq!(
                (hours[1].active_ms, hours[2].active_ms),
                (7_200_000, 7_200_000)
            );
        }
    }
}

#[test]
fn hourly_projection_is_bounded_and_conserves_partial_multi_day_records() {
    let timezone = chrono::FixedOffset::east_opt(5 * 3600 + 45 * 60).unwrap();
    let start = timezone
        .with_ymd_and_hms(2026, 1, 1, 23, 30, 0)
        .unwrap()
        .timestamp_millis();
    let end = start + 25 * 3_600_000;
    let source = snapshot(start, end);
    let hours = project_hours(&timezone, &source).unwrap();
    assert_eq!(
        hours.iter().map(|hour| hour.active_ms).sum::<i64>(),
        end - start
    );
    assert_eq!(
        (hours[23].active_ms, hours[0].active_ms),
        (5_400_000, 5_400_000)
    );
    let mut too_many = snapshot(0, 60_000);
    let record = too_many.records[0].clone();
    too_many.records = (0..=MAX_HISTORY_HOUR_CATEGORIES)
        .map(|index| ExactActivityRecord {
            category: format!("custom_{index}"),
            ..record.clone()
        })
        .collect();
    assert!(project_hours(&chrono::Utc, &too_many)
        .unwrap_err()
        .contains("category budget"));
    let mut too_much_work = snapshot(0, MAX_HISTORY_RANGE_MS);
    too_much_work.records = vec![too_much_work.records[0].clone(); 2000];
    assert!(project_hours(&chrono::Utc, &too_much_work)
        .unwrap_err()
        .contains("step budget"));
}

#[tokio::test]
async fn product_projection_uses_confirmed_records_and_excludes_imported_hour_quantities() {
    use sqlx::Executor;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
        .await
        .unwrap();
    pool.execute(crate::data::schema::ACTIVITY_IMPORT_SCHEMA_SQL)
        .await
        .unwrap();
    pool.execute("INSERT INTO sessions(app_name,exe_name,start_time,end_time) VALUES('Editor','editor',0,120000)").await.unwrap();
    sqlx::query("INSERT INTO import_batches(id,imported_at,source_name,source_kind,source_fingerprint,exact_session_count,hour_bucket_count) VALUES('fixture',0,'fixture','patina-csv',?,0,1)")
        .bind("a".repeat(64)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO import_time_buckets(batch_id,fingerprint,app_name,exe_name,bucket_start_time,duration) VALUES('fixture',?,'Bucket','must-not-appear',0,3600000)")
        .bind("c".repeat(64)).execute(&pool).await.unwrap();
    // Use the underlying read once to avoid competing with HTTP tests for the
    // production family's single-query admission slot.
    let history = super::super::read_snapshot(&pool, 0, 3_600_000, 3_600_000, "en-US")
        .await
        .unwrap();
    let hours = project_hours(&chrono::Local, &history).unwrap();
    assert_eq!(history.records.len(), 1);
    assert_eq!(hours.iter().map(|hour| hour.active_ms).sum::<i64>(), 120000);
    pool.close().await;
}
