use reqwest::{Method, StatusCode, Url};
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const MAX_TEXT_RESPONSE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebDavConfig {
    pub url: String,
    pub username: String,
    pub remote_dir: String,
}

pub struct WebDavClient {
    client: reqwest::Client,
    base_url: Url,
    username: String,
    password: String,
}

fn parse_base_url(raw: &str) -> Result<Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("WebDAV server address cannot be empty".to_string());
    }

    let mut url =
        Url::parse(trimmed).map_err(|error| format!("invalid WebDAV server address: {error}"))?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err("WebDAV server address must use http or https".to_string());
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

pub fn normalize_remote_dir(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let candidate = if trimmed.is_empty() {
        "/Patina"
    } else {
        trimmed
    };

    if candidate.contains('\\') || candidate.contains("..") {
        return Err("WebDAV remote directory contains unsupported path segments".to_string());
    }
    if candidate.chars().any(|char| char.is_control()) {
        return Err("WebDAV remote directory contains control characters".to_string());
    }

    let mut normalized = candidate.replace("//", "/");
    if !normalized.starts_with('/') {
        normalized = format!("/{normalized}");
    }
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }
    Ok(normalized)
}

fn split_path(path: &str) -> impl Iterator<Item = &str> {
    path.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
}

impl WebDavClient {
    pub fn new(config: &WebDavConfig, password: String) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| format!("failed to create WebDAV client: {error}"))?;

        Ok(Self {
            client,
            base_url: parse_base_url(&config.url)?,
            username: config.username.trim().to_string(),
            password,
        })
    }

    fn remote_url(&self, remote_path: &str) -> Result<Url, String> {
        let mut url = self.base_url.clone();
        let base_segments = url
            .path_segments()
            .map(|segments| {
                segments
                    .filter(|segment| !segment.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| "WebDAV server address cannot be used as a base URL".to_string())?;
            segments.clear();
            for segment in base_segments {
                segments.push(&segment);
            }
            for segment in split_path(remote_path) {
                segments.push(segment);
            }
        }

        Ok(url)
    }

    async fn request(
        &self,
        method: Method,
        remote_path: &str,
    ) -> Result<reqwest::RequestBuilder, String> {
        let url = self.remote_url(remote_path)?;
        Ok(self
            .client
            .request(method, url)
            .basic_auth(&self.username, Some(&self.password)))
    }

    pub async fn ping(&self, remote_dir: &str) -> Result<(), String> {
        self.ensure_dir(remote_dir).await
    }

    pub async fn ensure_dir(&self, remote_dir: &str) -> Result<(), String> {
        let normalized = normalize_remote_dir(remote_dir)?;
        let mut current = String::new();
        for segment in split_path(&normalized) {
            current.push('/');
            current.push_str(segment);
            let response = self
                .request(
                    Method::from_bytes(b"MKCOL").map_err(|error| error.to_string())?,
                    &current,
                )
                .await?
                .send()
                .await
                .map_err(|error| format!("failed to create WebDAV directory: {error}"))?;
            let status = response.status();
            if status == StatusCode::CREATED
                || status == StatusCode::METHOD_NOT_ALLOWED
                || status == StatusCode::OK
                || status == StatusCode::CONFLICT
            {
                continue;
            }
            return Err(format!("failed to create WebDAV directory: HTTP {status}"));
        }
        Ok(())
    }

    pub async fn read_text_optional(&self, remote_path: &str) -> Result<Option<String>, String> {
        let mut response = self
            .request(Method::GET, remote_path)
            .await?
            .send()
            .await
            .map_err(|error| format!("failed to read WebDAV file: {error}"))?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(format!("failed to read WebDAV file: HTTP {status}"));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_TEXT_RESPONSE_BYTES)
        {
            return Err("WebDAV text response exceeds the size limit".to_string());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| format!("failed to read WebDAV response: {error}"))?
        {
            let next_len = bytes
                .len()
                .checked_add(chunk.len())
                .ok_or_else(|| "WebDAV text response exceeds the size limit".to_string())?;
            if next_len as u64 > MAX_TEXT_RESPONSE_BYTES {
                return Err("WebDAV text response exceeds the size limit".to_string());
            }
            bytes.extend_from_slice(&chunk);
        }
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| "WebDAV text response is not valid UTF-8".to_string())
    }

    pub async fn write_text(&self, remote_path: &str, value: &str) -> Result<(), String> {
        let response = self
            .request(Method::PUT, remote_path)
            .await?
            .header("Content-Type", "application/json; charset=utf-8")
            .body(value.to_string())
            .send()
            .await
            .map_err(|error| format!("failed to write WebDAV file: {error}"))?;
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(format!("failed to write WebDAV file: HTTP {status}"))
        }
    }

    pub async fn upload_file(&self, local_path: &Path, remote_path: &str) -> Result<(), String> {
        let bytes = tokio::fs::read(local_path)
            .await
            .map_err(|error| format!("failed to read local backup before upload: {error}"))?;
        let response = self
            .request(Method::PUT, remote_path)
            .await?
            .header("Content-Type", "application/zip")
            .body(bytes)
            .send()
            .await
            .map_err(|error| format!("failed to upload WebDAV backup: {error}"))?;
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(format!("failed to upload WebDAV backup: HTTP {status}"))
        }
    }

    pub async fn download_file_bounded(
        &self,
        remote_path: &str,
        local_path: &Path,
        max_bytes: u64,
    ) -> Result<(), String> {
        let mut response = self
            .request(Method::GET, remote_path)
            .await?
            .send()
            .await
            .map_err(|error| format!("failed to download WebDAV backup: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("failed to download WebDAV backup: HTTP {status}"));
        }
        if response
            .content_length()
            .is_some_and(|length| length > max_bytes)
        {
            return Err("downloaded WebDAV backup exceeds the size limit".to_string());
        }

        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut file = options
            .open(local_path)
            .await
            .map_err(|error| format!("failed to create downloaded backup: {error}"))?;
        let result = async {
            let mut total = 0_u64;
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|error| format!("failed to read WebDAV backup response: {error}"))?
            {
                total = total
                    .checked_add(chunk.len() as u64)
                    .ok_or_else(|| "downloaded WebDAV backup exceeds the size limit".to_string())?;
                if total > max_bytes {
                    return Err("downloaded WebDAV backup exceeds the size limit".to_string());
                }
                file.write_all(&chunk)
                    .await
                    .map_err(|error| format!("failed to write downloaded backup: {error}"))?;
            }
            file.sync_all()
                .await
                .map_err(|error| format!("failed to sync downloaded backup: {error}"))
        }
        .await;
        drop(file);
        if result.is_err() {
            let _ = tokio::fs::remove_file(local_path).await;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_remote_dir, WebDavClient, WebDavConfig};

    fn temp_download_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "patina-webdav-{label}-{}-{}",
            std::process::id(),
            crate::app::runtime::now_ms()
        ))
    }

    async fn test_server(body: Vec<u8>) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let app = axum::Router::new().route(
            "/backup.zip",
            axum::routing::get(move || {
                let body = body.clone();
                async move { body }
            }),
        );
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), task)
    }

    #[test]
    fn normalize_remote_dir_applies_default_and_slashes() {
        assert_eq!(normalize_remote_dir("").unwrap(), "/Patina");
        assert_eq!(
            normalize_remote_dir("Patina/backups/").unwrap(),
            "/Patina/backups"
        );
    }

    #[test]
    fn normalize_remote_dir_rejects_unsafe_segments() {
        assert!(normalize_remote_dir("../zotero").is_err());
        assert!(normalize_remote_dir("Patina\\backups").is_err());
        assert!(normalize_remote_dir("Patina/\n/backups").is_err());
    }

    #[tokio::test]
    async fn bounded_download_rejects_oversized_responses_without_leaving_a_file() {
        let (url, task) = test_server(vec![7_u8; 16]).await;
        let client = WebDavClient::new(
            &WebDavConfig {
                url,
                username: "user".to_string(),
                remote_dir: "/Patina".to_string(),
            },
            "password".to_string(),
        )
        .unwrap();
        let target = temp_download_path("oversized");

        let result = client
            .download_file_bounded("/backup.zip", &target, 8)
            .await;

        assert!(result.is_err());
        assert!(!target.exists());
        task.abort();
    }

    #[tokio::test]
    async fn bounded_download_never_overwrites_an_existing_file() {
        let (url, task) = test_server(b"new backup".to_vec()).await;
        let client = WebDavClient::new(
            &WebDavConfig {
                url,
                username: "user".to_string(),
                remote_dir: "/Patina".to_string(),
            },
            "password".to_string(),
        )
        .unwrap();
        let target = temp_download_path("existing");
        std::fs::write(&target, b"keep").unwrap();

        let result = client
            .download_file_bounded("/backup.zip", &target, 1024)
            .await;

        assert!(result.is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"keep");
        std::fs::remove_file(target).unwrap();
        task.abort();
    }
}
