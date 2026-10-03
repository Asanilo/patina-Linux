use super::*;
use sqlx::QueryBuilder;

pub(super) struct Metadata {
    pub app_name: String,
    pub exe_name: String,
    pub title: String,
}

pub(super) async fn load(
    connection: &mut SqliteConnection,
    keys: &BTreeSet<(ExactActivityOrigin, i64)>,
) -> Result<BTreeMap<(ExactActivityOrigin, i64), Metadata>, String> {
    let mut result = BTreeMap::new();
    let mut bytes = 0;
    for (origin, table) in [
        (ExactActivityOrigin::Native, "sessions"),
        (ExactActivityOrigin::ImportExact, "import_exact_sessions"),
    ] {
        let ids = keys
            .iter()
            .filter_map(|(kind, id)| (*kind == origin).then_some(*id))
            .collect::<Vec<_>>();
        for chunk in ids.chunks(256) {
            let mut query = QueryBuilder::<sqlx::Sqlite>::new(format!(
                "SELECT id,
                    length(CAST(app_name AS BLOB)) AS app_bytes,
                    length(CAST(exe_name AS BLOB)) AS exe_bytes,
                    length(CAST(COALESCE(window_title,'') AS BLOB)) AS title_bytes,
                    CAST(substr(CAST(app_name AS BLOB),1,{name_limit}) AS TEXT) AS app_name,
                    CAST(substr(CAST(exe_name AS BLOB),1,{name_limit}) AS TEXT) AS exe_name,
                    CAST(substr(CAST(COALESCE(window_title,'') AS BLOB),1,{title_limit}) AS TEXT) AS title
                 FROM {table} WHERE id IN (",
                name_limit = MAX_HISTORY_NAME_BYTES + 1,
                title_limit = MAX_HISTORY_TITLE_BYTES + 1,
            ));
            let mut ids = query.separated(",");
            for id in chunk {
                ids.push_bind(id);
            }
            ids.push_unseparated(") ORDER BY id");
            let mut rows = query.build().fetch(&mut *connection);
            while let Some(row) = rows.try_next().await.map_err(query_error)? {
                for (field, maximum) in [
                    ("app_bytes", MAX_HISTORY_NAME_BYTES),
                    ("exe_bytes", MAX_HISTORY_NAME_BYTES),
                    ("title_bytes", MAX_HISTORY_TITLE_BYTES),
                ] {
                    let size: i64 = row.try_get(field).map_err(query_error)?;
                    if size < 0 || size as usize > maximum {
                        return Err("exact history metadata field budget exceeded".into());
                    }
                    bytes += size as usize;
                }
                if bytes > MAX_HISTORY_RESPONSE_BYTES {
                    return Err("exact history metadata budget exceeded".into());
                }
                result.insert(
                    (origin, row.try_get("id").map_err(query_error)?),
                    Metadata {
                        app_name: row.try_get("app_name").map_err(query_error)?,
                        exe_name: row.try_get("exe_name").map_err(query_error)?,
                        title: row.try_get("title").map_err(query_error)?,
                    },
                );
            }
        }
    }
    Ok(result)
}

pub(super) async fn load_samples(
    connection: &mut SqliteConnection,
    ids: &BTreeSet<i64>,
    from_ms: i64,
    to_ms: i64,
    cutoff: i64,
) -> Result<BTreeMap<i64, Vec<ExactTitleSample>>, String> {
    let mut result = BTreeMap::<i64, Vec<ExactTitleSample>>::new();
    let keys = ids.iter().copied().collect::<Vec<_>>();
    let mut count = 0;
    let mut bytes = 0;
    for chunk in keys.chunks(256) {
        let mut query = QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT s.session_id,s.start_time,COALESCE(s.end_time,p.end_time,",
        );
        query.push_bind(cutoff).push(format!(
            ") AS effective_end,
                length(CAST(s.title AS BLOB)) AS title_bytes,
                CAST(substr(CAST(s.title AS BLOB),1,{title_limit}) AS TEXT) AS title
             FROM session_title_samples s JOIN sessions p ON p.id=s.session_id
             WHERE s.session_id IN (",
            title_limit = MAX_HISTORY_TITLE_BYTES + 1,
        ));
        let mut selected = query.separated(",");
        for id in chunk {
            selected.push_bind(id);
        }
        selected.push_unseparated(") AND s.start_time < ");
        query
            .push_bind(to_ms)
            .push(" AND COALESCE(s.end_time,p.end_time,")
            .push_bind(cutoff)
            .push(") > ")
            .push_bind(from_ms)
            .push(" ORDER BY s.session_id,s.start_time,s.id LIMIT ")
            .push_bind((MAX_HISTORY_TITLE_SAMPLES - count + 1) as i64);
        let mut rows = query.build().fetch(&mut *connection);
        while let Some(row) = rows.try_next().await.map_err(query_error)? {
            count += 1;
            let size: i64 = row.try_get("title_bytes").map_err(query_error)?;
            if count > MAX_HISTORY_TITLE_SAMPLES
                || size < 0
                || size as usize > MAX_HISTORY_TITLE_BYTES
            {
                return Err("exact history title sample budget exceeded".into());
            }
            bytes += size as usize;
            if bytes > MAX_HISTORY_RESPONSE_BYTES {
                return Err("exact history title bytes exceeded".into());
            }
            let start: i64 = row.try_get("start_time").map_err(query_error)?;
            let end: i64 = row.try_get("effective_end").map_err(query_error)?;
            if [start, end]
                .iter()
                .any(|v| !(-MAX_SAFE_TIMESTAMP..=MAX_SAFE_TIMESTAMP).contains(v))
            {
                return Err("invalid title sample timestamp".into());
            }
            let sample = ExactTitleSample {
                title: row.try_get("title").map_err(query_error)?,
                start_ms: start.max(from_ms),
                end_ms: end.min(to_ms),
            };
            if sample.end_ms > sample.start_ms {
                result
                    .entry(row.try_get("session_id").map_err(query_error)?)
                    .or_default()
                    .push(sample);
            }
        }
    }
    Ok(result)
}
