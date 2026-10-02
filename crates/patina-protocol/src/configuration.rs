//! Bounded classification configuration, retaining the existing namespaced keys
//! during reader migration. No arbitrary settings or credential access.
use serde::{Deserialize, Serialize};

pub const MAX_CLASSIFICATION_ENTRIES: usize = 20_000;
pub const MAX_CLASSIFICATION_KEY_BYTES: usize = 256;
pub const MAX_CLASSIFICATION_VALUE_BYTES: usize = 4096;
pub const MAX_CLASSIFICATION_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_CLASSIFICATION_MUTATIONS: usize = 256;
pub const CLASSIFICATION_PREFIXES: &[&str] = &[
    "__app_override::",
    "__web_domain_override::",
    "__category_color_override::",
    "__category_label_override::",
    "__category_default_color_assignment::",
    "__custom_category::",
    "__deleted_category::",
    "__classification_manual_confirmation_migration::",
];

pub fn is_classification_key(key: &str) -> bool {
    key.len() <= MAX_CLASSIFICATION_KEY_BYTES
        && CLASSIFICATION_PREFIXES
            .iter()
            .any(|prefix| key.starts_with(prefix) && key.len() > prefix.len())
}

pub fn is_revision(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClassificationEntry {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClassificationSnapshot {
    pub revision: String,
    pub sampled_at_ms: i64,
    pub entries: Vec<ClassificationEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClassificationMutationRequest {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClassificationMutationsRequest {
    pub mutations: Vec<ClassificationMutationRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClassificationCommitResult {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}
