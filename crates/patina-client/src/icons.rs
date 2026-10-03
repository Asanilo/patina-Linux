use crate::{Client, ClientError};
use patina_protocol::icons::*;
use std::time::Duration;
impl Client {
    pub async fn icon_page(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<IconPage, ClientError> {
        if limit == 0
            || limit > MAX_ICON_PAGE_ENTRIES
            || after.is_some_and(|s| s.len() > MAX_ICON_KEY_BYTES)
        {
            return Err(ClientError::InvalidConfiguration(
                "invalid icon cursor or limit".into(),
            ));
        }
        let mut url = reqwest::Url::parse("http://localhost/").expect("fixed URL");
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("limit", &limit.to_string());
            if let Some(after) = after {
                query.append_pair("after", after);
            }
        }
        let page: IconPage = self
            .get_json_with_limits(
                &format!("/api/v1/assets/icons?{}", url.query().unwrap_or("")),
                "icon page",
                Duration::from_secs(10),
                MAX_ICON_PAGE_BYTES,
            )
            .await?;
        validate_page(&page, after, limit)?;
        Ok(page)
    }
    pub async fn cached_icon(&self, key: &str) -> Result<IconLookup, ClientError> {
        if key.is_empty() || key.len() > MAX_ICON_KEY_BYTES {
            return Err(ClientError::InvalidConfiguration("invalid icon key".into()));
        }
        let mut url = reqwest::Url::parse("http://localhost/").expect("fixed URL");
        url.query_pairs_mut().append_pair("key", key);
        let result: IconLookup = self
            .get_json_with_limits(
                &format!("/api/v1/assets/icon?{}", url.query().unwrap_or("")),
                "cached icon",
                Duration::from_secs(10),
                MAX_ICON_LOOKUP_BYTES,
            )
            .await?;
        if result.requested_key != key || result.icon.as_ref().is_some_and(|icon| !valid_icon(icon))
        {
            return Err(ClientError::InvalidResponse(
                "invalid cached icon response".into(),
            ));
        }
        Ok(result)
    }
}
fn validate_page(page: &IconPage, after: Option<&str>, limit: usize) -> Result<(), ClientError> {
    let error = || ClientError::InvalidResponse("invalid cached icon page".into());
    if page.entries.len() > limit {
        return Err(error());
    }
    let mut previous = after;
    for entry in &page.entries {
        if !valid_icon(entry) || previous.is_some_and(|p| entry.source_key.as_str() <= p) {
            return Err(error());
        }
        previous = Some(&entry.source_key);
    }
    if let Some(next) = page.next_after.as_deref() {
        if page.entries.last().map(|entry| entry.source_key.as_str()) != Some(next)
            || after.is_some_and(|last| next <= last)
        {
            return Err(error());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn icon_pages_must_advance_and_contain_only_bounded_inline_assets() {
        let icon = CachedIcon {
            source_key: "editor".into(),
            keys: vec!["editor".into()],
            data_url: "data:image/png;base64,AAAA".into(),
        };
        let page = IconPage {
            entries: vec![icon.clone()],
            next_after: Some("editor".into()),
        };
        assert!(validate_page(&page, None, 64).is_ok());
        assert!(validate_page(&page, Some("editor"), 64).is_err());
        assert!(validate_page(
            &IconPage {
                entries: vec![],
                next_after: Some("editor".into())
            },
            None,
            64
        )
        .is_err());
        assert!(validate_page(
            &IconPage {
                entries: vec![icon.clone(), icon.clone()],
                next_after: None
            },
            None,
            64
        )
        .is_err());
        let mut invalid = page.clone();
        invalid.entries[0].data_url = "file:///etc/passwd".into();
        assert!(validate_page(&invalid, None, 64).is_err());
        invalid = page;
        invalid.entries[0].keys.push("editor".into());
        assert!(validate_page(&invalid, None, 64).is_err());
    }
}
