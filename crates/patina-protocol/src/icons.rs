//! Read-only cached presentation assets; no filesystem paths or extraction requests.
use serde::{Deserialize, Serialize};
pub const MAX_ICON_KEY_BYTES: usize = 1024;
pub const MAX_ICON_DATA_BYTES: usize = 512 * 1024;
pub const MAX_ICON_LOOKUP_BYTES: usize = MAX_ICON_DATA_BYTES + 64 * 1024;
pub const MAX_ICON_PAGE_ENTRIES: usize = 64;
pub const MAX_ICON_PAGE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_ICON_CATALOG_ENTRIES: usize = 4096;
pub const MAX_ICON_CATALOG_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_ICON_CATALOG_PAGES: usize = 128;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedIcon {
    pub source_key: String,
    pub keys: Vec<String>,
    pub data_url: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IconPage {
    pub entries: Vec<CachedIcon>,
    pub next_after: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IconLookup {
    pub requested_key: String,
    pub icon: Option<CachedIcon>,
}
pub fn valid_icon_data(value: &str) -> bool {
    if value.len() > MAX_ICON_DATA_BYTES {
        return false;
    }
    let Some(body) = value
        .strip_prefix("data:image/png;base64,")
        .or_else(|| value.strip_prefix("data:image/svg+xml;base64,"))
    else {
        return false;
    };
    if body.len() % 4 != 0 || body.len() - body.trim_end_matches('=').len() > 2 {
        return false;
    }
    let body = body.trim_end_matches('=');
    !body.is_empty()
        && body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
}
pub fn valid_icon(icon: &CachedIcon) -> bool {
    icon.source_key.len() <= MAX_ICON_KEY_BYTES
        && !icon.source_key.is_empty()
        && icon.keys.len() <= 3
        && icon
            .keys
            .iter()
            .all(|key| !key.is_empty() && key.len() <= MAX_ICON_KEY_BYTES)
        && icon
            .keys
            .iter()
            .enumerate()
            .all(|(i, key)| !icon.keys[..i].contains(key))
        && valid_icon_data(&icon.data_url)
}
