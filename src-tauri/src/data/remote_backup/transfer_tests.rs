use super::*;
use crate::data::repositories::backup_restore::test_support as fixture;
use axum::{
    body::Body,
    http::{HeaderMap, Method, StatusCode, Uri},
    response::Response,
    Router,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[tokio::test]
async fn isolated_webdav_restore_download_validates_before_staging() {
    let root = std::env::temp_dir().join(format!(
        "patina-webdav-transfer-{}",
        random_download_file_name().unwrap()
    ));
    fs::create_dir(&root).unwrap();
    ensure_temp_backup_dir(&root).unwrap();
    let pool = crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path(
        &root.join("data/patina.db"),
        true,
    )
    .await
    .unwrap();
    fixture::seed_session(&pool, 1, "synthetic", 1000).await;
    let source = root.join("synthetic.zip");
    backup::export_scheduled_backup_create_new(&pool, &source)
        .await
        .unwrap();
    let original = fs::read(&source).unwrap();
    let (preview, _, size) = backup::inspect_restore_archive(&source).unwrap();
    let entry = build_entry(
        "safe-id".into(),
        remote_backup_file_name("safe-id"),
        "/Patina/Patina-backup-safe-id.zip".into(),
        size,
        &preview,
    );

    for mode in [
        "success",
        "unsafe-path",
        "duplicate-id",
        "missing-index",
        "unauthorized",
        "corrupt-archive",
        "wrong-metadata",
        "missing-archive",
    ] {
        let temp = root.join(mode).join("downloads");
        let staging = root.join(mode).join("staging");
        ensure_temp_backup_dir(&temp).unwrap();
        ensure_temp_backup_dir(&staging).unwrap();
        fs::write(temp.join("unrelated.txt"), b"keep-download").unwrap();
        fs::write(staging.join("unrelated.txt"), b"keep-staging").unwrap();
        let mut item = entry.clone();
        if mode == "unsafe-path" {
            item.remote_path = "/other/private.zip".into();
        }
        if mode == "wrong-metadata" {
            item.session_count += 1;
        }
        let mut index = empty_index();
        index.backups.push(item.clone());
        if mode == "duplicate-id" {
            index.backups.push(item);
        }
        let index_bytes = serde_json::to_vec(&index).unwrap();
        let bytes = original.clone();
        let downloads = Arc::new(AtomicUsize::new(0));
        let count = downloads.clone();
        let router = Router::new().fallback(move |method: Method, uri: Uri, headers: HeaderMap| {
            let index = index_bytes.clone();
            let bytes = bytes.clone();
            let count = count.clone();
            async move {
                assert_eq!(method, Method::GET);
                assert_eq!(
                    headers.get("authorization").unwrap(),
                    "Basic dXNlcjpwYXNzd29yZA=="
                );
                let (status, body) = match uri.path() {
                    "/Patina/backup-index.json" => match mode {
                        "missing-index" => (StatusCode::NOT_FOUND, Vec::new()),
                        "unauthorized" => (StatusCode::UNAUTHORIZED, Vec::new()),
                        _ => (StatusCode::OK, index),
                    },
                    "/Patina/Patina-backup-safe-id.zip" => {
                        count.fetch_add(1, Ordering::SeqCst);
                        match mode {
                            "missing-archive" => (StatusCode::NOT_FOUND, Vec::new()),
                            "corrupt-archive" => (StatusCode::OK, b"not a ZIP".to_vec()),
                            _ => (StatusCode::OK, bytes),
                        }
                    }
                    _ => panic!("unexpected remote request"),
                };
                Response::builder()
                    .status(status)
                    .body(Body::from(body))
                    .unwrap()
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let client = WebDavClient::new(
            &WebDavConfig {
                url,
                username: "user".into(),
                remote_dir: "/Patina".into(),
            },
            "password".into(),
        )
        .unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            stage_webdav_backup_with_client(&client, "/Patina", "safe-id", &temp, &staging),
        )
        .await;
        task.abort();
        let _ = task.await;
        let result = result.unwrap();
        assert_eq!(result.is_ok(), mode == "success", "{mode}: {result:?}");
        let fetched = matches!(
            mode,
            "success" | "corrupt-archive" | "wrong-metadata" | "missing-archive"
        );
        assert_eq!(
            downloads.load(Ordering::SeqCst),
            usize::from(fetched),
            "{mode}"
        );
        if let Ok(staged) = result {
            let path = crate::platform::backup_restore_staging::validate(
                &staging,
                &staged.ticket,
                &staged.sha256,
                staged.size_bytes,
            )
            .unwrap();
            assert_eq!(fs::read(&path).unwrap(), original);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            crate::platform::backup_restore_staging::discard(&staging, &staged.ticket).unwrap();
        }
        assert_eq!(fs::read_dir(&temp).unwrap().count(), 1, "{mode}");
        assert_eq!(fs::read_dir(&staging).unwrap().count(), 1, "{mode}");
        assert_eq!(
            fs::read(temp.join("unrelated.txt")).unwrap(),
            b"keep-download"
        );
        assert_eq!(
            fs::read(staging.join("unrelated.txt")).unwrap(),
            b"keep-staging"
        );
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(
            fixture::session_names(&pool).await,
            vec!["synthetic".to_string()]
        );
        assert_eq!(fixture::receipt_count(&pool).await, 0);
        fixture::assert_integrity(&pool).await;
    }
    pool.close().await;
    fs::remove_dir_all(root).unwrap();
}
