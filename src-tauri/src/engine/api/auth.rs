use std::sync::OnceLock;

static API_TOKEN: OnceLock<String> = OnceLock::new();

#[allow(dead_code)]
pub fn set_api_token(token: String) {
    let _ = API_TOKEN.set(token);
}

pub fn get_api_token() -> &'static str {
    API_TOKEN.get_or_init(|| {
        // Try to load from file first, fall back to generating new one
        load_or_generate_token()
    })
}

pub fn token_file_path() -> std::path::PathBuf {
    let data_dir = std::env::var("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            std::path::PathBuf::from(home).join(".local/share")
        });
    data_dir.join("Patina").join("api_token")
}

fn load_or_generate_token() -> String {
    let path = token_file_path();
    if let Ok(token) = std::fs::read_to_string(&path) {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return token;
        }
    }
    let token = generate_random_token();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let _ = std::fs::write(&path, &token);
    token
}

pub fn validate_token(authorization: Option<&str>) -> bool {
    let expected = get_api_token();
    let Some(auth_header) = authorization else {
        return false;
    };

    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);

    token == expected
}

fn generate_random_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    format!("patina_api_{timestamp:x}")
}
