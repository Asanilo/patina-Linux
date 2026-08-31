use crate::domain::activity_import::{
    CanonicalImportRecord, ImportRecordType, ImportRowError, ParsedCanonicalCsv,
    MAX_APP_IDENTIFIER_BYTES, MAX_APP_NAME_BYTES, MAX_CATEGORY_BYTES, MAX_IMPORT_RECORDS,
    MAX_WINDOW_TITLE_BYTES,
};
use chrono::DateTime;
use serde::Deserialize;

const REQUIRED_HEADERS: &[&str] = &[
    "record_type",
    "start_time",
    "end_time",
    "duration_ms",
    "exe_name",
    "app_name",
    "title",
    "category",
];

#[derive(Debug, Deserialize)]
struct CanonicalCsvRow {
    record_type: ImportRecordType,
    start_time: String,
    end_time: String,
    duration_ms: String,
    exe_name: String,
    app_name: String,
    title: String,
    category: String,
}

pub fn parse_canonical_csv(bytes: &[u8]) -> Result<ParsedCanonicalCsv, String> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| "Patina CSV must be UTF-8 encoded".to_string())?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(false)
        .from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|error| format!("failed to read Patina CSV header: {error}"))?
        .clone();
    validate_headers(&headers)?;

    let mut parsed = ParsedCanonicalCsv::default();
    for (index, result) in reader.deserialize::<CanonicalCsvRow>().enumerate() {
        let line = index + 2;
        if parsed.records.len() + parsed.errors.len() >= MAX_IMPORT_RECORDS {
            return Err(format!(
                "Patina CSV exceeds the {MAX_IMPORT_RECORDS} record safety limit"
            ));
        }
        match result {
            Ok(row) => match validate_row(line, row) {
                Ok(record) => parsed.records.push(record),
                Err(message) => parsed.errors.push(ImportRowError { line, message }),
            },
            Err(error) => parsed.errors.push(ImportRowError {
                line,
                message: format!("CSV row could not be parsed: {error}"),
            }),
        }
    }
    Ok(parsed)
}

fn validate_headers(headers: &csv::StringRecord) -> Result<(), String> {
    if headers.iter().eq(REQUIRED_HEADERS.iter().copied()) {
        Ok(())
    } else {
        Err(format!(
            "invalid Patina CSV columns; expected {}",
            REQUIRED_HEADERS.join(",")
        ))
    }
}

fn validate_row(line: usize, row: CanonicalCsvRow) -> Result<CanonicalImportRecord, String> {
    let start_time_ms = parse_rfc3339(&row.start_time, "start_time")?;
    let duration_ms = row
        .duration_ms
        .trim()
        .parse::<i64>()
        .map_err(|_| "duration_ms must be an integer".to_string())?;
    if duration_ms <= 0 {
        return Err("duration_ms must be positive".to_string());
    }
    let end_time_ms = optional_text(&row.end_time)
        .map(|value| parse_rfc3339(&value, "end_time"))
        .transpose()?;

    match row.record_type {
        ImportRecordType::ExactSession => {
            let end = end_time_ms.ok_or_else(|| "exact_session requires end_time".to_string())?;
            if end <= start_time_ms {
                return Err("exact_session end_time must be after start_time".to_string());
            }
            if (end - start_time_ms).abs_diff(duration_ms) > 1_000 {
                return Err("exact_session duration_ms must match start_time/end_time".to_string());
            }
        }
        ImportRecordType::HourBucket => {
            if end_time_ms.is_some() {
                return Err("hour_bucket end_time must be empty".to_string());
            }
            if duration_ms > 3_600_000 {
                return Err("hour_bucket duration_ms cannot exceed one hour".to_string());
            }
            if optional_unprotected_text(&row.title).is_some() {
                return Err("hour_bucket title must be empty".to_string());
            }
        }
    }

    let exe_name = unprotect_spreadsheet_text(&row.exe_name).trim().to_string();
    if exe_name.is_empty()
        || exe_name.len() > MAX_APP_IDENTIFIER_BYTES
        || exe_name.contains(['\r', '\n', '\0'])
    {
        return Err("exe_name must be a non-empty application identifier".to_string());
    }
    let app_name = optional_unprotected_text(&row.app_name);
    let title = optional_unprotected_text(&row.title);
    let category = optional_unprotected_text(&row.category);
    validate_optional_length(&app_name, MAX_APP_NAME_BYTES, "app_name")?;
    validate_optional_length(&title, MAX_WINDOW_TITLE_BYTES, "title")?;
    validate_optional_length(&category, MAX_CATEGORY_BYTES, "category")?;

    Ok(CanonicalImportRecord {
        source_line: line,
        record_type: row.record_type,
        start_time_ms,
        end_time_ms,
        duration_ms,
        exe_name,
        app_name,
        title,
        category,
    })
}

fn validate_optional_length(
    value: &Option<String>,
    max_bytes: usize,
    field: &str,
) -> Result<(), String> {
    if let Some(value) = value {
        if value.len() > max_bytes {
            return Err(format!("{field} exceeds the {max_bytes} byte limit"));
        }
        if value.contains('\0') {
            return Err(format!("{field} cannot contain NUL characters"));
        }
    }
    Ok(())
}

fn parse_rfc3339(value: &str, field: &str) -> Result<i64, String> {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|timestamp| timestamp.timestamp_millis())
        .map_err(|_| format!("{field} must be an RFC 3339 timestamp with an offset"))
}

fn optional_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn optional_unprotected_text(value: &str) -> Option<String> {
    optional_text(&unprotect_spreadsheet_text(value))
}

fn unprotect_spreadsheet_text(value: &str) -> String {
    if value.starts_with('\'') && has_spreadsheet_formula_prefix(&value[1..]) {
        value[1..].to_string()
    } else {
        value.to_string()
    }
}

fn has_spreadsheet_formula_prefix(value: &str) -> bool {
    matches!(
        value
            .trim_start_matches([' ', '\t', '\r', '\n'])
            .as_bytes()
            .first(),
        Some(b'=' | b'+' | b'-' | b'@')
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str =
        "record_type,start_time,end_time,duration_ms,exe_name,app_name,title,category\n";

    #[test]
    fn accepts_linux_and_windows_application_identifiers() {
        let csv = format!(
            "{HEADER}exact_session,2026-01-15T09:00:00+08:00,2026-01-15T09:30:00+08:00,1800000,org.gnome.Terminal,Terminal,Shell,Development\n\
             hour_bucket,2026-01-15T10:00:00+08:00,,600000,code.exe,Code,,Development\n"
        );
        let parsed = parse_canonical_csv(csv.as_bytes()).unwrap();
        assert_eq!(parsed.records.len(), 2);
        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.records[0].exe_name, "org.gnome.Terminal");
    }

    #[test]
    fn rejects_invalid_exact_duration_and_hour_title() {
        let csv = format!(
            "{HEADER}exact_session,2026-01-15T09:00:00Z,2026-01-15T09:30:00Z,1,app,App,Title,\n\
             hour_bucket,2026-01-15T10:00:00Z,,1000,app,App,Not allowed,\n"
        );
        let parsed = parse_canonical_csv(csv.as_bytes()).unwrap();
        assert!(parsed.records.is_empty());
        assert_eq!(parsed.errors.len(), 2);
    }

    #[test]
    fn requires_exact_portable_header() {
        let error = parse_canonical_csv(b"start_time,exe_name\n1,app\n").unwrap_err();
        assert!(error.contains("invalid Patina CSV columns"));

        let spaced = format!(" record_type,{}", &HEADER["record_type,".len()..]);
        assert!(parse_canonical_csv(spaced.as_bytes()).is_err());
    }
}
