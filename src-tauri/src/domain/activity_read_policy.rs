//! Historical read filtering, deliberately separate from live tracking eligibility.
//! Keep parity with processNormalization.ts until all readers use the backend.

use serde::Deserialize;
use std::{collections::HashMap, sync::OnceLock};

const LIFECYCLE: &[&str] = &[
    "setup",
    "install",
    "installer",
    "uninstall",
    "uninstaller",
    "unins",
    "unins000",
    "update",
    "updater",
    "upgrade",
    "remove",
    "maintenance",
    "maintenancetool",
];
const BUILD: &[&str] = &[
    "win", "windows", "x64", "x86", "amd64", "arm64", "ia32", "portable", "release", "latest",
    "beta", "alpha", "nightly", "stable", "desktop", "app",
];
const STANDALONE: &[&str] = &[
    "geek",
    "geekuninstaller",
    "revouninstaller",
    "revouninstallerpro",
    "iobituninstaller",
    "hibituninstaller",
    "bcuninstaller",
    "bulkcrapuninstaller",
    "uninstalr",
];
const BLOCKED: &[&str] = &[
    "",
    "msiexec",
    "uninstall",
    "unins000",
    "unins",
    "un_a",
    "hrupdate",
    "consent",
    "csrss",
    "dwm",
    "fontdrvhost",
    "gameinputsvc",
    "logonui",
    "lsass",
    "runtimebroker",
    "services",
    "sihost",
    "smss",
    "system",
    "svchost",
    "usoclient",
    "wininit",
    "winlogon",
    "wuauclt",
    "applicationframehost",
    "lockapp",
    "openwith",
    "pickerhost",
    "searchhost",
    "shellexperiencehost",
    "startmenuexperiencehost",
    "taskhostw",
    "textinputhost",
];

#[derive(Deserialize)]
struct CatalogEntry {
    name: String,
}

fn catalog() -> &'static HashMap<String, CatalogEntry> {
    static CATALOG: OnceLock<HashMap<String, CatalogEntry>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../src/shared/classification/defaultMappings.json"
        ))
        .expect("embedded app catalog must be valid")
    })
}

fn trim_js(value: &str) -> &str {
    value.trim_matches(|ch| {
        matches!(ch,
            '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
            '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
            '\u{205f}' | '\u{3000}' | '\u{feff}'
        )
    })
}

fn normalize(exe: &str) -> String {
    trim_js(exe).to_lowercase().trim_matches('"').to_string()
}

fn stem(exe: &str) -> &str {
    exe.strip_suffix(".exe").unwrap_or(exe)
}

fn identity(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

fn separator(ch: char) -> bool {
    matches!(ch, '_' | '-' | '.' | ' ')
}

fn tokens(value: &str) -> Vec<&str> {
    value
        .split(separator)
        .filter(|token| !token.is_empty())
        .collect()
}

fn version(token: &str) -> bool {
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|ch| ch.is_ascii_digit());
    if digits(token) {
        return true;
    }
    let parts: Vec<_> = token
        .strip_prefix('v')
        .unwrap_or(token)
        .split('.')
        .collect();
    (2..=6).contains(&parts.len()) && parts.into_iter().all(digits)
}

fn compact_lifecycle(value: &str) -> Option<(&str, &str)> {
    LIFECYCLE.iter().find_map(|marker| {
        let base = value.strip_suffix(marker)?;
        (base.chars().count() >= 2 && base.bytes().any(|ch| ch.is_ascii_lowercase()))
            .then_some((base, *marker))
    })
}

fn equivalent_stems(left: &str, right: &str) -> bool {
    left == right
        || match (
            catalog().get(&format!("{left}.exe")),
            catalog().get(&format!("{right}.exe")),
        ) {
            (Some(left), Some(right)) => left.name == right.name,
            _ => false,
        }
}

fn metadata_signal(value: &str) -> bool {
    let value = value.to_lowercase();
    if [
        "\u{5b89}\u{88c5}",
        "\u{5378}\u{8f7d}",
        "\u{66f4}\u{65b0}",
        "\u{7ef4}\u{62a4}\u{5de5}\u{5177}",
    ]
    .iter()
    .any(|word| value.contains(word))
    {
        return true;
    }
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|token| {
            LIFECYCLE.contains(&token)
                || [
                    "installation",
                    "installing",
                    "uninstallation",
                    "uninstalling",
                    "updating",
                ]
                .contains(&token)
        })
}

fn explicitly_blocked(value: &str) -> bool {
    BLOCKED.contains(&value)
        || value
            .strip_suffix(".exe")
            .is_some_and(|base| !matches!(base, "" | "system") && BLOCKED.contains(&base))
}

fn alias_base(value: &str) -> Option<String> {
    let candidate = tokens(value)
        .into_iter()
        .filter(|part| !BUILD.contains(part) && !LIFECYCLE.contains(part) && !version(part))
        .collect::<Vec<_>>()
        .join("-");
    (candidate.len() >= 2 && candidate.bytes().any(|ch| ch.is_ascii_lowercase()))
        .then_some(candidate)
}

/// Read-side identity, matching the Desktop mapper rather than live process identity.
pub fn canonical_executable(exe: &str) -> String {
    let normalized = normalize(exe);
    let name = stem(&normalized);
    for suffix in ["webhelper", "helper", "widget", "tray"] {
        if let Some(base) = name.strip_suffix(suffix) {
            let base = base.trim_end_matches(['_', '-', '.']);
            if !base.is_empty() && catalog().contains_key(&format!("{base}.exe")) {
                return format!("{base}.exe");
            }
        }
    }
    // Find the first separated lifecycle marker, as in Desktop's suffix pattern.
    for (offset, ch) in name.char_indices().filter(|(_, ch)| separator(*ch)) {
        if offset == 0 {
            continue;
        }
        let tail = &name[offset + ch.len_utf8()..];
        let marker = tail.split(separator).next().unwrap_or("");
        if LIFECYCLE.contains(&marker) {
            if let Some(base) = alias_base(&name[..offset]) {
                return format!("{base}.exe");
            }
            break;
        }
    }
    if let Some(offset) = name.find(separator) {
        if LIFECYCLE.contains(&&name[..offset]) {
            if let Some(base) = alias_base(&name[offset + 1..]) {
                return format!("{base}.exe");
            }
        }
    }
    let parts = tokens(name);
    if parts.len() >= 2
        && parts.iter().any(|part| version(part))
        && parts
            .iter()
            .any(|part| BUILD.contains(part) || LIFECYCLE.contains(part))
    {
        if let Some(base) = alias_base(parts[0]) {
            return format!("{base}.exe");
        }
    }
    normalized
}

pub fn needs_metadata(exe: &str) -> bool {
    let normalized = normalize(exe);
    let name = stem(&normalized);
    let parts = tokens(name);
    matches!(name, "launcher")
        || compact_lifecycle(name).is_some()
        || (parts.len() >= 2
            && parts.iter().any(|part| version(part))
            && parts.iter().any(|part| BUILD.contains(part)))
}

/// Metadata is inspected transiently; callers must retain only the decision.
pub fn should_include_fact(exe: &str, app_name: &str, title: &str) -> bool {
    let normalized = normalize(exe);
    let name = stem(&normalized);
    if normalized.ends_with(".tmp") {
        return false;
    }
    let parts = tokens(name);
    let metadata = [app_name, title];
    if !STANDALONE.contains(&identity(name).as_str()) {
        if LIFECYCLE.contains(&name)
            || (parts.len() >= 2 && parts.iter().any(|part| LIFECYCLE.contains(part)))
        {
            return false;
        }
        if let Some((base, marker)) = compact_lifecycle(name) {
            if metadata.iter().any(|value| {
                let value = identity(value);
                value == name
                    || compact_lifecycle(&value).is_some_and(|(other_base, other_marker)| {
                        marker == other_marker && equivalent_stems(base, other_base)
                    })
            }) {
                return false;
            }
        }
    }
    if parts.len() >= 2
        && parts.iter().any(|part| version(part))
        && parts.iter().any(|part| BUILD.contains(part))
        && metadata.iter().any(|value| metadata_signal(value))
    {
        return false;
    }
    if matches!(normalized.as_str(), "launcher" | "launcher.exe")
        && metadata
            .iter()
            .any(|value| identity(value) == "wallpaperenginelauncher")
    {
        return false;
    }

    let canonical = canonical_executable(exe);
    !(explicitly_blocked(&canonical)
        || canonical
            .strip_suffix(".exe")
            .is_some_and(explicitly_blocked))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_alias_fixture_matches_desktop_identity() {
        let cases: Vec<(String, String)> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/activity-read-alias-cases.json"
        ))
        .unwrap();
        for (exe, expected) in cases {
            assert_eq!(canonical_executable(&exe), expected, "{exe:?}");
        }
    }

    #[test]
    fn shared_fixture_matches_desktop_historical_filter() {
        let cases: Vec<(String, String, String, bool)> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/activity-read-filter-cases.json"
        ))
        .unwrap();
        for (exe, app, title, expected) in cases {
            if !needs_metadata(&exe) {
                assert_eq!(
                    should_include_fact(&exe, "", ""),
                    expected,
                    "metadata gate: {exe:?}"
                );
            }
            assert_eq!(
                should_include_fact(&exe, &app, &title),
                expected,
                "{exe:?}, {app:?}, {title:?}"
            );
        }
    }
}
