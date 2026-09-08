#[cfg(target_os = "windows")]
const WEBDAV_BACKUP_CREDENTIAL_TARGET: &str = "com.ceceliaee.patina.backup.webdav.default";

#[cfg(target_os = "windows")]
mod windows_credentials {
    use super::WEBDAV_BACKUP_CREDENTIAL_TARGET;
    use std::ptr;
    use windows::core::PWSTR;
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn is_not_found(error: &windows::core::Error) -> bool {
        error.code() == ERROR_NOT_FOUND.to_hresult()
    }

    fn credential_target(profile_key: &str) -> String {
        if profile_key == "production" {
            WEBDAV_BACKUP_CREDENTIAL_TARGET.to_string()
        } else {
            format!("{WEBDAV_BACKUP_CREDENTIAL_TARGET}.{profile_key}")
        }
    }

    pub fn save_webdav_password(
        profile_key: &str,
        username: &str,
        password: &str,
    ) -> Result<(), String> {
        let mut target = wide_null(&credential_target(profile_key));
        let mut comment = wide_null("Patina WebDAV backup credential");
        let mut username = wide_null(username);
        let mut password_bytes = password.as_bytes().to_vec();

        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_mut_ptr()),
            Comment: PWSTR(comment.as_mut_ptr()),
            CredentialBlobSize: password_bytes
                .len()
                .try_into()
                .map_err(|_| "WebDAV password is too large to store".to_string())?,
            CredentialBlob: password_bytes.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: PWSTR(username.as_mut_ptr()),
            ..Default::default()
        };

        unsafe {
            CredWriteW(&credential, 0)
                .map_err(|error| format!("failed to save WebDAV credential: {error}"))
        }
    }

    pub fn read_webdav_password(profile_key: &str) -> Result<Option<String>, String> {
        let target = wide_null(&credential_target(profile_key));
        let mut credential: *mut CREDENTIALW = ptr::null_mut();

        let result = unsafe {
            CredReadW(
                windows::core::PCWSTR(target.as_ptr()),
                CRED_TYPE_GENERIC,
                None,
                &mut credential,
            )
        };

        match result {
            Ok(()) => {
                if credential.is_null() {
                    return Ok(None);
                }

                let secret = unsafe {
                    let credential_ref = &*credential;
                    let bytes = std::slice::from_raw_parts(
                        credential_ref.CredentialBlob,
                        credential_ref.CredentialBlobSize as usize,
                    );
                    let secret = String::from_utf8(bytes.to_vec())
                        .map_err(|_| "stored WebDAV credential is not valid UTF-8".to_string())?;
                    CredFree(credential.cast());
                    secret
                };
                Ok(Some(secret))
            }
            Err(error) if is_not_found(&error) => Ok(None),
            Err(error) => Err(format!("failed to read WebDAV credential: {error}")),
        }
    }

    pub fn delete_webdav_password(profile_key: &str) -> Result<(), String> {
        let target = wide_null(&credential_target(profile_key));
        let result = unsafe {
            CredDeleteW(
                windows::core::PCWSTR(target.as_ptr()),
                CRED_TYPE_GENERIC,
                None,
            )
        };

        match result {
            Ok(()) => Ok(()),
            Err(error) if is_not_found(&error) => Ok(()),
            Err(error) => Err(format!("failed to delete WebDAV credential: {error}")),
        }
    }
}

#[cfg(target_os = "linux")]
mod linux_credentials {
    use secret_service::{EncryptionType, SecretService};
    use std::collections::HashMap;

    const CREDENTIAL_LABEL: &str = "Patina WebDAV backup credential";
    const APPLICATION_ATTRIBUTE: &str = "com.ceceliaee.patina";
    const PURPOSE_ATTRIBUTE: &str = "webdav-backup";
    fn lookup_attributes(profile_key: &str) -> HashMap<&str, &str> {
        HashMap::from([
            ("application", APPLICATION_ATTRIBUTE),
            ("purpose", PURPOSE_ATTRIBUTE),
            ("profile", profile_key),
        ])
    }

    async fn connect() -> Result<SecretService<'static>, String> {
        SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(|error| format!("failed to connect to Linux Secret Service: {error}"))
    }

    pub async fn save_webdav_password(
        profile_key: &str,
        username: &str,
        password: &str,
    ) -> Result<(), String> {
        let service = connect().await?;
        let collection = service
            .get_default_collection()
            .await
            .map_err(|error| format!("failed to open the default Linux keyring: {error}"))?;
        if collection
            .is_locked()
            .await
            .map_err(|error| format!("failed to inspect the default Linux keyring: {error}"))?
        {
            collection
                .unlock()
                .await
                .map_err(|error| format!("failed to unlock the default Linux keyring: {error}"))?;
        }
        let label = format!("{CREDENTIAL_LABEL} ({username})");
        collection
            .create_item(
                &label,
                lookup_attributes(profile_key),
                password.as_bytes(),
                true,
                "text/plain; charset=utf-8",
            )
            .await
            .map_err(|error| {
                format!("failed to save WebDAV credential in Linux keyring: {error}")
            })?;
        Ok(())
    }

    pub async fn read_webdav_password(profile_key: &str) -> Result<Option<String>, String> {
        let service = connect().await?;
        let mut items = service
            .search_items(lookup_attributes(profile_key))
            .await
            .map_err(|error| format!("failed to search Linux keyring: {error}"))?;
        let item = if let Some(item) = items.unlocked.pop() {
            item
        } else if let Some(item) = items.locked.pop() {
            item.unlock()
                .await
                .map_err(|error| format!("failed to unlock WebDAV credential: {error}"))?;
            item
        } else {
            return Ok(None);
        };
        let secret = item.get_secret().await.map_err(|error| {
            format!("failed to read WebDAV credential from Linux keyring: {error}")
        })?;
        String::from_utf8(secret)
            .map(Some)
            .map_err(|_| "stored WebDAV credential is not valid UTF-8".to_string())
    }

    pub async fn has_webdav_password(profile_key: &str) -> Result<bool, String> {
        let service = connect().await?;
        let items = service
            .search_items(lookup_attributes(profile_key))
            .await
            .map_err(|error| format!("failed to search Linux keyring: {error}"))?;
        Ok(!items.unlocked.is_empty() || !items.locked.is_empty())
    }

    pub async fn delete_webdav_password(profile_key: &str) -> Result<(), String> {
        let service = connect().await?;
        let items = service
            .search_items(lookup_attributes(profile_key))
            .await
            .map_err(|error| format!("failed to search Linux keyring: {error}"))?;
        for item in items.unlocked.into_iter().chain(items.locked) {
            if item
                .is_locked()
                .await
                .map_err(|error| format!("failed to inspect WebDAV credential: {error}"))?
            {
                item.unlock()
                    .await
                    .map_err(|error| format!("failed to unlock WebDAV credential: {error}"))?;
            }
            item.delete()
                .await
                .map_err(|error| format!("failed to delete WebDAV credential: {error}"))?;
        }
        Ok(())
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod unsupported_credentials {
    pub async fn save_webdav_password(
        _profile_key: &str,
        _username: &str,
        _password: &str,
    ) -> Result<(), String> {
        Err("WebDAV credential storage is unavailable on this platform".to_string())
    }

    pub async fn read_webdav_password(_profile_key: &str) -> Result<Option<String>, String> {
        Ok(None)
    }

    pub async fn has_webdav_password(_profile_key: &str) -> Result<bool, String> {
        Ok(false)
    }

    pub async fn delete_webdav_password(_profile_key: &str) -> Result<(), String> {
        Ok(())
    }
}

pub async fn save_webdav_backup_password(
    profile: crate::platform::app_paths::AppProfile,
    username: &str,
    password: &str,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    return windows_credentials::save_webdav_password(profile.key(), username, password);
    #[cfg(target_os = "linux")]
    return linux_credentials::save_webdav_password(profile.key(), username, password).await;
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    return unsupported_credentials::save_webdav_password(profile.key(), username, password).await;
}

pub async fn read_webdav_backup_password(
    profile: crate::platform::app_paths::AppProfile,
) -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    return windows_credentials::read_webdav_password(profile.key());
    #[cfg(target_os = "linux")]
    return linux_credentials::read_webdav_password(profile.key()).await;
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    return unsupported_credentials::read_webdav_password(profile.key()).await;
}

pub async fn has_webdav_backup_password(
    profile: crate::platform::app_paths::AppProfile,
) -> Result<bool, String> {
    #[cfg(target_os = "linux")]
    return linux_credentials::has_webdav_password(profile.key()).await;
    #[cfg(target_os = "windows")]
    return Ok(windows_credentials::read_webdav_password(profile.key())?.is_some());
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    return unsupported_credentials::has_webdav_password(profile.key()).await;
}

pub async fn delete_webdav_backup_password(
    profile: crate::platform::app_paths::AppProfile,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    return windows_credentials::delete_webdav_password(profile.key());
    #[cfg(target_os = "linux")]
    return linux_credentials::delete_webdav_password(profile.key()).await;
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    return unsupported_credentials::delete_webdav_password(profile.key()).await;
}
