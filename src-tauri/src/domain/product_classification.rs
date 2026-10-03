//! Product read semantics. Presentation colors and translated labels stay with clients.
use super::activity_read_policy::{canonical_executable, default_category, trim_js};
use patina_protocol::configuration::ClassificationEntry;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const ASSIGNABLE: &[&str] = &[
    "ai",
    "development",
    "office",
    "browser",
    "communication",
    "video",
    "music",
    "game",
    "design",
    "utility",
    "other",
];

#[derive(Debug, Default)]
struct Override {
    category: Option<String>,
    display_name: Option<String>,
    excluded: bool,
}

#[derive(Debug, Default)]
pub struct ProductClassification {
    overrides: HashMap<String, Override>,
    deleted: HashSet<String>,
}

impl ProductClassification {
    /// Entries are ordered by storage key, like the owner configuration snapshot.
    /// Invalid legacy overrides remain ignored; reads never migrate them.
    pub fn from_entries(entries: &[ClassificationEntry], language: &str) -> Self {
        let mut policy = Self::default();
        for entry in entries {
            if let Some(category) = entry.key.strip_prefix("__deleted_category::") {
                if ASSIGNABLE.contains(&category) && category != "other" {
                    policy.deleted.insert(category.into());
                }
            } else if let Some(exe) = entry.key.strip_prefix("__app_override::") {
                let key = canonical_executable(exe);
                if !key.is_empty() {
                    if let Some(value) = parse_override(&entry.value, language) {
                        policy.overrides.insert(key, value);
                    }
                }
            }
        }
        policy
    }

    pub fn category(&self, canonical: &str) -> &str {
        let category = self
            .overrides
            .get(canonical)
            .and_then(|value| value.category.as_deref())
            .unwrap_or_else(|| default_category(canonical));
        if self.deleted.contains(category) {
            ASSIGNABLE
                .iter()
                .copied()
                .find(|value| !self.deleted.contains(*value))
                .unwrap_or("other")
        } else {
            category
        }
    }

    pub fn excludes(&self, canonical: &str) -> bool {
        self.category(canonical) == "system"
            || self
                .overrides
                .get(canonical)
                .is_some_and(|value| value.excluded)
    }

    pub fn display_name_override(&self, canonical: &str) -> Option<&str> {
        self.overrides
            .get(canonical)
            .and_then(|value| value.display_name.as_deref())
    }
}

fn parse_override(raw: &str, language: &str) -> Option<Override> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let object = value.as_object()?;
    if object.get("enabled") == Some(&Value::Bool(false)) {
        return None;
    }
    // The legacy JS reader rejects the whole object when a typed string method
    // is invoked on a non-string field. Preserve that boundary for stored data.
    for key in ["category", "displayName", "color"] {
        if object
            .get(key)
            .is_some_and(|v| !v.is_null() && !v.is_string())
        {
            return None;
        }
    }
    let category = normalize_category(
        object.get("category").and_then(Value::as_str).unwrap_or(""),
        language,
    )
    .ok()?;
    let display_name = object
        .get("displayName")
        .and_then(Value::as_str)
        .map(trim_js)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let excluded = object.get("track") == Some(&Value::Bool(false));
    let color = object
        .get("color")
        .and_then(Value::as_str)
        .map(trim_js)
        .unwrap_or("");
    let color = color.strip_prefix('#').unwrap_or(color);
    let has_color = color.len() == 6 && color.bytes().all(|b| b.is_ascii_hexdigit());
    (category.is_some()
        || display_name.is_some()
        || excluded
        || has_color
        || object.get("captureTitle") == Some(&Value::Bool(false)))
    .then_some(Override {
        category,
        display_name,
        excluded,
    })
}

fn normalize_category(raw: &str, language: &str) -> Result<Option<String>, ()> {
    let raw = trim_js(raw);
    if ASSIGNABLE.contains(&raw) {
        return Ok(Some(raw.into()));
    }
    let Some(label) = raw.strip_prefix("custom:").filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let mut label = label.to_owned();
    for _ in 0..4 {
        // decodeURIComponent rejects malformed escapes and UTF-8, without changing '+'.
        let bytes = label.as_bytes();
        if bytes.iter().enumerate().any(|(i, b)| {
            *b == b'%'
                && (i + 2 >= bytes.len()
                    || !bytes[i + 1].is_ascii_hexdigit()
                    || !bytes[i + 2].is_ascii_hexdigit())
        }) {
            break;
        }
        let Ok(decoded) = percent_encoding::percent_decode_str(&label).decode_utf8() else {
            break;
        };
        if decoded == label {
            break;
        }
        label = decoded.into_owned();
    }
    let compact = label
        .split(|ch: char| trim_js(&ch.to_string()).is_empty())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let compact = if compact.is_empty() {
        if language == "zh-CN" {
            "自定义"
        } else {
            "Custom"
        }
    } else {
        &compact
    };
    // JS slices 20 UTF-16 units. A split surrogate makes encodeURIComponent throw
    // and the legacy override reader ignores the whole object.
    let units = compact.encode_utf16().take(20).collect::<Vec<_>>();
    let label = String::from_utf16(&units).map_err(|_| ())?;
    let mut encoded = String::new();
    for byte in label.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            encoded.push(byte as char);
        } else {
            use std::fmt::Write;
            write!(&mut encoded, "%{byte:02X}").unwrap();
        }
    }
    Ok(Some(format!("custom:{encoded}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_fixture_matches_desktop_product_semantics() {
        let cases: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/product-classification.json"
        ))
        .unwrap();
        for fixture in cases.as_array().unwrap() {
            let mut entries = Vec::new();
            for entry in fixture["overrides"].as_array().unwrap() {
                entries.push(ClassificationEntry {
                    key: format!("__app_override::{}", entry["exe"].as_str().unwrap()),
                    value: entry["value"].to_string(),
                });
            }
            for category in fixture["deleted"].as_array().unwrap() {
                entries.push(ClassificationEntry {
                    key: format!("__deleted_category::{}", category.as_str().unwrap()),
                    value: "1".into(),
                });
            }
            entries.sort_by(|a, b| a.key.cmp(&b.key));
            let policy = ProductClassification::from_entries(
                &entries,
                fixture["language"].as_str().unwrap(),
            );
            for check in fixture["checks"].as_array().unwrap() {
                let exe = canonical_executable(check["exe"].as_str().unwrap());
                assert_eq!(
                    policy.category(&exe),
                    check["category"].as_str().unwrap(),
                    "{}",
                    fixture["name"]
                );
                assert_eq!(
                    !policy.excludes(&exe)
                        && super::super::activity_read_policy::should_include_fact(&exe, "", ""),
                    check["tracked"].as_bool().unwrap()
                );
                assert_eq!(policy.display_name_override(&exe), check["name"].as_str());
            }
        }
    }
}
