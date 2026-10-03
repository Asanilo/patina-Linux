use crate::domain::activity_read_policy::{canonical_executable, trim_js};
use futures_util::TryStreamExt;
use patina_protocol::icons::*;
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::time::Duration;

fn aliases(source: &str) -> Vec<String> {
    let raw = trim_js(source);
    let mut result = Vec::new();
    for key in [
        raw.to_owned(),
        raw.to_lowercase(),
        canonical_executable(raw),
    ] {
        if !key.is_empty() && !result.contains(&key) {
            result.push(key);
        }
    }
    result
}
fn error(e: sqlx::Error) -> String {
    format!("cached icon read failed: {e}")
}
fn text(row: &sqlx::sqlite::SqliteRow, field: &str, max: usize) -> Result<String, String> {
    let raw: Vec<u8> = row.try_get(field).map_err(error)?;
    if raw.len() > max {
        return Err("cached icon field budget exceeded".into());
    }
    String::from_utf8(raw).map_err(|_| "invalid cached icon encoding".into())
}
fn icon(row: &sqlx::sqlite::SqliteRow) -> Result<CachedIcon, String> {
    let source_key = text(row, "exe_name", MAX_ICON_KEY_BYTES)?;
    let data_url = text(row, "icon_base64", MAX_ICON_DATA_BYTES)?;
    let entry = CachedIcon {
        keys: aliases(&source_key),
        source_key,
        data_url,
    };
    if !valid_icon(&entry) {
        return Err("invalid cached icon data or aliases".into());
    }
    Ok(entry)
}

pub async fn load_page(
    pool: &SqlitePool,
    after: Option<&str>,
    limit: usize,
) -> Result<IconPage, String> {
    if limit == 0
        || limit > MAX_ICON_PAGE_ENTRIES
        || after.is_some_and(|s| s.len() > MAX_ICON_KEY_BYTES)
    {
        return Err("invalid cached icon cursor or limit".into());
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut connection = pool.acquire().await.map_err(error)?;
        page(&mut connection, after, limit).await
    })
    .await
    .map_err(|_| "cached icon page exceeded its time budget".to_owned())?
}
async fn page(
    connection: &mut SqliteConnection,
    after: Option<&str>,
    limit: usize,
) -> Result<IconPage, String> {
    let mut rows = sqlx::query(
        "SELECT substr(CAST(exe_name AS BLOB),1,1025) AS exe_name,
        substr(CAST(icon_base64 AS BLOB),1,524289) AS icon_base64 FROM icon_cache
        WHERE length(CAST(exe_name AS BLOB))>0 AND (?1 IS NULL OR exe_name COLLATE BINARY>?1)
        ORDER BY exe_name COLLATE BINARY LIMIT ?2",
    )
    .bind(after)
    .bind((limit + 1) as i64)
    .fetch(connection);
    let mut entries = Vec::new();
    let mut bytes = 16 * 1024;
    let mut next_after = None;
    while let Some(row) = rows.try_next().await.map_err(error)? {
        if entries.len() == limit {
            next_after = entries
                .last()
                .map(|item: &CachedIcon| item.source_key.clone());
            break;
        }
        let entry = icon(&row)?;
        let encoded = serde_json::to_vec(&entry).map_err(|e| e.to_string())?.len() + 1;
        if bytes + encoded > MAX_ICON_PAGE_BYTES {
            next_after = entries
                .last()
                .map(|item: &CachedIcon| item.source_key.clone());
            if next_after.is_none() {
                return Err("cached icon page budget exceeded".into());
            }
            break;
        }
        bytes += encoded;
        entries.push(entry);
    }
    Ok(IconPage {
        entries,
        next_after,
    })
}

pub async fn lookup(pool: &SqlitePool, key: &str) -> Result<IconLookup, String> {
    if key.is_empty() || key.len() > MAX_ICON_KEY_BYTES {
        return Err("invalid cached icon key".into());
    }
    tokio::time::timeout(Duration::from_secs(5),async {
        let canonical=canonical_executable(key);
        let mut tx=pool.begin().await.map_err(error)?;
        let mut rows=sqlx::query("SELECT substr(CAST(exe_name AS BLOB),1,1025) AS exe_name FROM icon_cache WHERE length(CAST(exe_name AS BLOB))>0 ORDER BY exe_name COLLATE BINARY LIMIT ?")
            .bind((MAX_ICON_CATALOG_ENTRIES+1) as i64).fetch(&mut *tx);
        let mut count=0;let mut selected=None;
        while let Some(row)=rows.try_next().await.map_err(error)? {
            count+=1;if count>MAX_ICON_CATALOG_ENTRIES {return Err("cached icon lookup entry budget exceeded".into());}
            let source=text(&row,"exe_name",MAX_ICON_KEY_BYTES)?;
            if aliases(&source).contains(&canonical) {selected=Some(source);}
        }
        drop(rows);
        let result=if let Some(source)=selected {
            let row=sqlx::query("SELECT CAST(exe_name AS BLOB) AS exe_name,substr(CAST(icon_base64 AS BLOB),1,524289) AS icon_base64 FROM icon_cache WHERE exe_name=?")
                .bind(source).fetch_one(&mut *tx).await.map_err(error)?;
            Some(icon(&row)?)
        }else{None};
        tx.commit().await.map_err(error)?;
        Ok(IconLookup {requested_key:key.to_owned(),icon:result})
    }).await.map_err(|_|"cached icon lookup exceeded its time budget".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor;
    async fn database() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool
    }
    async fn insert(pool: &SqlitePool, key: &str, value: &str) {
        super::super::upsert_icon(pool, key, value, 0)
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn icon_pages_and_lookup_share_aliases_without_filesystem_fallback() {
        let pool = database().await;
        for key in [" Cursor.exe ", "a&?#.exe", "z.exe", "应用"] {
            insert(&pool, key, "data:image/png;base64,AAAA").await;
        }
        let first = load_page(&pool, None, 2).await.unwrap();
        assert_eq!(first.entries.len(), 2);
        assert_eq!(first.next_after.as_deref(), Some("a&?#.exe"));
        assert!(first.entries[0].keys.contains(&"cursor.exe".into()));
        let second = load_page(&pool, first.next_after.as_deref(), 2)
            .await
            .unwrap();
        assert_eq!(second.entries.len(), 2);
        assert!(second.next_after.is_none());
        assert_eq!(
            lookup(&pool, "cursor.exe").await.unwrap().icon,
            Some(first.entries[0].clone())
        );
        assert!(lookup(&pool, "/etc/passwd").await.unwrap().icon.is_none());
        pool.close().await;
    }
    #[tokio::test]
    async fn icon_byte_budget_paginates_without_skipping_the_next_record() {
        let pool = database().await;
        let value = format!("data:image/png;base64,{}", "A".repeat(500000));
        for index in 0..7 {
            insert(&pool, &format!("icon{index}"), &value).await;
        }
        let first = load_page(&pool, None, 64).await.unwrap();
        assert_eq!(first.entries.len(), 4);
        let second = load_page(&pool, first.next_after.as_deref(), 64)
            .await
            .unwrap();
        assert_eq!(second.entries.len(), 3);
        assert_eq!(second.entries[0].source_key, "icon4");
        insert(
            &pool,
            "zz",
            &format!("data:image/png;base64,{}", "A".repeat(MAX_ICON_DATA_BYTES)),
        )
        .await;
        assert!(load_page(&pool, Some("icon6"), 64)
            .await
            .unwrap_err()
            .contains("budget"));
        pool.close().await;
    }
}
