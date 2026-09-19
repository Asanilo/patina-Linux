use super::*;
use crate::domain::daily_activity::DailyAppIdentity;
use sqlx::QueryBuilder;

#[derive(Default)]
pub(super) struct IdentityCollector {
    values: HashMap<Arc<str>, Candidate>,
}

struct Candidate {
    value: DailyAppIdentity,
    first: (i64, u8, i64),
    name_order: (i64, u8, i64),
}

fn score(name: &str) -> u8 {
    let name = name.trim();
    let lower = name.to_lowercase();
    if name.is_empty() {
        0
    } else if lower.contains("tray") || lower.contains("widget") {
        1
    } else if name
        .chars()
        .any(|ch| ('\u{3400}'..='\u{9fff}').contains(&ch))
    {
        4
    } else if lower.contains('_') || lower.contains('-') {
        2
    } else {
        3
    }
}

impl IdentityCollector {
    pub(super) async fn read(
        &mut self,
        connection: &mut SqliteConnection,
        requests: Vec<(ActivityOrigin, i64, Arc<str>)>,
    ) -> Result<(), String> {
        // Fetch only contributing metadata in bounded primary-key batches, never
        // titles or a second whole-range snapshot. All reads share the day transaction.
        for (origin, source, table, time) in [
            (
                ActivityOrigin::ImportBucket,
                0,
                "import_time_buckets",
                "bucket_start_time",
            ),
            (
                ActivityOrigin::ImportExact,
                1,
                "import_exact_sessions",
                "start_time",
            ),
            (ActivityOrigin::Native, 2, "sessions", "start_time"),
        ] {
            let ids: HashMap<_, _> = requests
                .iter()
                .filter(|(kind, _, _)| *kind == origin)
                .map(|(_, id, key)| (*id, key.clone()))
                .collect();
            let keys: Vec<_> = ids.keys().copied().collect();
            for chunk in keys.chunks(256) {
                let mut query = QueryBuilder::<sqlx::Sqlite>::new(format!("SELECT id, {time} AS start, substr(COALESCE(app_name,''),1,1025) AS name, substr(exe_name,1,1025) AS exe FROM {table} WHERE id IN ("));
                let mut separated = query.separated(",");
                for id in chunk {
                    separated.push_bind(id);
                }
                separated.push_unseparated(")");
                let mut rows = query.build().fetch(&mut *connection);
                while let Some(row) = rows.try_next().await.map_err(query_error)? {
                    let id: i64 = row.try_get("id").map_err(query_error)?;
                    let key = ids.get(&id).ok_or("unexpected application metadata row")?;
                    let name: String = row.try_get("name").map_err(query_error)?;
                    let exe: String = row.try_get("exe").map_err(query_error)?;
                    if name.len() > MAX_APP_KEY_BYTES || exe.len() > MAX_APP_KEY_BYTES {
                        return Err("daily application identity exceeds budget".into());
                    }
                    let order = (
                        row.try_get::<i64, _>("start").map_err(query_error)?,
                        source,
                        id,
                    );
                    let raw_key = exe.trim().to_lowercase().trim_matches('"').to_string();
                    let exe_name = if raw_key == key.as_ref() {
                        exe
                    } else {
                        key.to_string()
                    };
                    let app_name = if raw_key == key.as_ref() {
                        name.trim().to_string()
                    } else {
                        String::new()
                    };
                    if let Some(current) = self.values.get_mut(key) {
                        if order < current.first {
                            current.first = order;
                            current.value.exe_name = exe_name;
                        }
                        if score(&app_name) > score(&current.value.app_name)
                            || (score(&app_name) == score(&current.value.app_name)
                                && order < current.name_order)
                        {
                            current.name_order = order;
                            current.value.app_name = app_name;
                        }
                    } else {
                        self.values.insert(
                            key.clone(),
                            Candidate {
                                value: DailyAppIdentity {
                                    app_key: key.to_string(),
                                    app_name,
                                    exe_name,
                                },
                                first: order,
                                name_order: order,
                            },
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Vec<DailyAppIdentity> {
        let mut values: Vec<_> = self.values.into_values().collect();
        values.sort_by(|a, b| {
            a.first
                .cmp(&b.first)
                .then_with(|| a.value.app_key.cmp(&b.value.app_key))
        });
        values
            .into_iter()
            .map(|candidate| candidate.value)
            .collect()
    }
}
