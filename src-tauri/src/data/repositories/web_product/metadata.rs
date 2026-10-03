use super::query_error;
use futures_util::TryStreamExt;
use patina_protocol::web_history::*;
use sqlx::{QueryBuilder, Row, Sqlite, SqliteConnection};
use std::collections::HashMap;

pub(super) struct Metadata {
    pub browser_client_id: String,
    pub browser_kind: String,
    pub browser_exe_name: String,
    pub domain: String,
    pub normalized_domain: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub favicon_url: Option<String>,
}
pub(super) async fn load(
    connection: &mut SqliteConnection,
    ids: &[i64],
    privacy: WebActivityUrlPrivacyMode,
) -> Result<HashMap<i64, Metadata>, String> {
    let mut result = HashMap::new();
    let mut bytes = 0;
    for batch in ids.chunks(128) {
        let mut query = QueryBuilder::<Sqlite>::new("SELECT id");
        for (field, limit) in [
            ("browser_client_id", MAX_WEB_HISTORY_NAME_BYTES),
            ("browser_kind", MAX_WEB_HISTORY_NAME_BYTES),
            ("browser_exe_name", MAX_WEB_HISTORY_NAME_BYTES),
            ("domain", MAX_WEB_HISTORY_NAME_BYTES),
            ("normalized_domain", MAX_WEB_HISTORY_NAME_BYTES),
            ("title", MAX_WEB_HISTORY_TITLE_BYTES),
            ("favicon_url", MAX_WEB_HISTORY_ICON_BYTES),
            ("url", MAX_WEB_HISTORY_URL_BYTES),
        ] {
            if field == "url" && privacy == WebActivityUrlPrivacyMode::DomainOnly {
                query.push(",NULL AS url");
            } else {
                query.push(format!(
                    ",substr(CAST({field} AS BLOB),1,{}) AS {field}",
                    limit + 1
                ));
            }
        }
        query.push(" FROM web_activity_segments WHERE id IN (");
        let mut separated = query.separated(",");
        for id in batch {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
        let mut rows = query.build().fetch(&mut *connection);
        while let Some(row) = rows.try_next().await.map_err(query_error)? {
            let mut string = |field: &str, limit: usize| -> Result<Option<String>, String> {
                let raw: Option<Vec<u8>> = row.try_get(field).map_err(query_error)?;
                raw.map(|value| {
                    bytes += value.len();
                    if value.len() > limit || bytes > MAX_WEB_HISTORY_RESPONSE_BYTES {
                        return Err("web history metadata budget exceeded".into());
                    }
                    String::from_utf8(value).map_err(|_| "invalid web history text".into())
                })
                .transpose()
            };
            let browser_client_id = string("browser_client_id", MAX_WEB_HISTORY_NAME_BYTES)?
                .ok_or("missing browser client")?;
            let browser_kind = string("browser_kind", MAX_WEB_HISTORY_NAME_BYTES)?
                .ok_or("missing browser kind")?;
            let browser_exe_name = string("browser_exe_name", MAX_WEB_HISTORY_NAME_BYTES)?
                .ok_or("missing browser executable")?;
            let raw_domain = string("domain", MAX_WEB_HISTORY_NAME_BYTES)?.unwrap_or_default();
            let normalized_domain = string("normalized_domain", MAX_WEB_HISTORY_NAME_BYTES)?
                .and_then(|v| crate::domain::web_activity::normalize_domain(&v))
                .ok_or("missing web domain")?;
            if browser_client_id.is_empty()
                || browser_kind.is_empty()
                || browser_exe_name.is_empty()
            {
                return Err("missing browser source identity".into());
            }
            let domain = if raw_domain.trim().is_empty() {
                normalized_domain.clone()
            } else {
                raw_domain
            };
            result.insert(
                row.try_get("id").map_err(query_error)?,
                Metadata {
                    browser_client_id,
                    browser_kind,
                    browser_exe_name,
                    domain,
                    normalized_domain,
                    url: string("url", MAX_WEB_HISTORY_URL_BYTES)?,
                    title: string("title", MAX_WEB_HISTORY_TITLE_BYTES)?,
                    favicon_url: string("favicon_url", MAX_WEB_HISTORY_ICON_BYTES)?,
                },
            );
        }
    }
    Ok(result)
}
