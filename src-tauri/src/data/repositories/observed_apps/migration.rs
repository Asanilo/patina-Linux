//! Full-history evidence for the one-time legacy classification migration.
use super::*;

const MAX_MIGRATION_FACTS: usize = 1_000_000;

pub async fn load_migration_observed_apps(
    pool: &SqlitePool,
    to_ms: i64,
    sampled_at_ms: i64,
) -> Result<Vec<ObservedAppStat>, String> {
    if to_ms <= 0 || to_ms > sampled_at_ms {
        return Err("migration evidence requires a positive cutoff no later than now".into());
    }
    let _permit = QUERY
        .try_acquire()
        .map_err(|_| "observed apps query is busy")?;
    tokio::time::timeout(Duration::from_secs(30), load(pool, to_ms, sampled_at_ms))
        .await
        .map_err(|_| "migration evidence exceeded its time budget".to_string())?
}

async fn load(pool: &SqlitePool, to: i64, sampled: i64) -> Result<Vec<ObservedAppStat>, String> {
    let mut tx = pool.begin().await.map_err(query_error)?;
    let active_end = sampled.min(to);
    let query = format!("{FACTS_SQL} ORDER BY start_time, source, id LIMIT ?");
    let mut rows = sqlx::query(&query)
        .bind(active_end)
        .bind(active_end)
        .bind(to)
        .bind(active_end)
        .bind(0)
        .bind(to)
        .bind(0)
        .bind(HOUR_MS)
        .bind(to)
        .bind(-HOUR_MS)
        .bind((MAX_MIGRATION_FACTS + 1) as i64)
        .fetch(&mut *tx);
    let mut component = Component::default();
    let mut stats = StatAccumulator::default();
    let mut count = 0;
    while let Some(row) = rows.try_next().await.map_err(query_error)? {
        count += 1;
        if count > MAX_MIGRATION_FACTS {
            return Err("migration evidence facts exceed budget".into());
        }
        let start: i64 = row.try_get("start_time").map_err(query_error)?;
        // Disjoint capacity components cannot influence native/import precedence
        // or hourly allocation. Never split a bucket's full capacity interval.
        if !component.facts.is_empty() && start > component.end {
            component.flush(&mut stats, to)?;
        }
        component.push(row, start)?;
    }
    drop(rows);
    component.flush(&mut stats, to)?;
    tx.commit().await.map_err(query_error)?;
    stats.finish()
}

#[derive(Default)]
struct Component {
    end: i64,
    bytes: usize,
    facts: Vec<(OwnedActivityRange<usize>, i64, String, String)>,
}

impl Component {
    fn push(&mut self, row: sqlx::sqlite::SqliteRow, start: i64) -> Result<(), String> {
        if self.facts.len() == MAX_FACTS {
            return Err("migration evidence overlapping facts exceed budget".into());
        }
        let exe: String = row.try_get("exe_name").map_err(query_error)?;
        let name: String = row.try_get("app_name").map_err(query_error)?;
        self.bytes += exe.len() + name.len();
        if exe.len() > MAX_TEXT_BYTES
            || name.len() > MAX_TEXT_BYTES
            || self.bytes > MAX_METADATA_BYTES
        {
            return Err("migration evidence metadata exceeds budget".into());
        }
        let end: i64 = row.try_get("end_time").map_err(query_error)?;
        let capacity: i64 = row.try_get("capacity_end").map_err(query_error)?;
        self.end = self.end.max(capacity).max(end).max(start);
        let origin = match row.try_get::<i64, _>("source").map_err(query_error)? {
            0 => ActivityOrigin::Native,
            1 => ActivityOrigin::ImportExact,
            _ => ActivityOrigin::ImportBucket,
        };
        self.facts.push((
            OwnedActivityRange {
                origin,
                start_ms: start,
                end_ms: end,
                capacity_end_ms: Some(capacity),
                value: 0,
            },
            row.try_get("id").map_err(query_error)?,
            exe,
            name,
        ));
        Ok(())
    }

    fn flush(&mut self, stats: &mut StatAccumulator, to: i64) -> Result<(), String> {
        // Restore the existing source/id tie order before using the shared compiler.
        self.facts
            .sort_by_key(|(fact, id, _, _)| (fact.origin, *id));
        let mut records = Vec::with_capacity(self.facts.len());
        let mut metadata = Vec::with_capacity(self.facts.len());
        for (mut fact, _, exe, name) in self.facts.drain(..) {
            fact.value = metadata.len();
            records.push(fact);
            metadata.push((exe, name));
        }
        stats.append(&records, &metadata, 0, to)?;
        self.end = 0;
        self.bytes = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor;

    #[tokio::test]
    async fn component_stream_matches_whole_snapshot_across_sources_and_old_history() {
        let cases: Vec<super::super::tests::Case> = serde_json::from_str(include_str!(
            "../../../../../tests/fixtures/observed-apps.json"
        ))
        .unwrap();
        for case in cases {
            let pool = super::super::tests::setup().await;
            for (index, fact) in case.facts.iter().enumerate() {
                super::super::tests::insert(&pool, index, fact).await;
            }
            let expected = load_snapshot(&pool, 0, case.to, case.sampled, MAX_FACTS)
                .await
                .unwrap();
            assert_eq!(
                load(&pool, case.to, case.sampled).await.unwrap(),
                expected,
                "{}",
                case.name
            );
        }
        let pool = super::super::tests::setup().await;
        pool.execute("WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i<59999) INSERT INTO sessions(app_name,exe_name,start_time,end_time,duration) SELECT 'Old Editor','editor',i*86400000,i*86400000+10,10 FROM n").await.unwrap();
        let to = 60_000 * 86_400_000_i64;
        let result = load(&pool, to, to).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].total_duration_ms, 600_000);
        assert_eq!(result[0].last_seen_ms, 59_999 * 86_400_000_i64);
    }

    #[tokio::test]
    async fn migration_limits_fail_without_writing_or_marking_completion() {
        let pool = super::super::tests::setup().await;
        pool.execute("WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM n WHERE i<50000) INSERT INTO sessions(app_name,exe_name,start_time,end_time,duration) SELECT 'Editor','editor',1,10,9 FROM n").await.unwrap();
        assert!(load(&pool, 20, 20)
            .await
            .unwrap_err()
            .contains("overlapping facts"));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 50001);
        let settings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(settings, 0);
        assert!(load_migration_observed_apps(&pool, 30, 20).await.is_err());
        pool.execute("DELETE FROM sessions").await.unwrap();
        pool.execute("INSERT INTO sessions(app_name,exe_name,start_time,end_time,duration) VALUES('Editor','editor',1,2,1)").await.unwrap();
        sqlx::query("UPDATE sessions SET app_name=?")
            .bind("x".repeat(1025))
            .execute(&pool)
            .await
            .unwrap();
        assert!(load(&pool, 20, 20).await.unwrap_err().contains("metadata"));
    }
}
