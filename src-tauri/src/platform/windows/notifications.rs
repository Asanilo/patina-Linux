use std::path::PathBuf;

use tauri_winrt_notification::Toast;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};

pub fn send(
    app_id: &str,
    app_name: &str,
    title: &str,
    body: &str,
    icon_path: Option<PathBuf>,
) -> Result<(), String> {
    register_app_user_model_id(app_id, app_name, icon_path)?;
    Toast::new(app_id)
        .title(title)
        .text1(body)
        .show()
        .map_err(|error| error.to_string())
}

fn register_app_user_model_id(
    app_id: &str,
    app_name: &str,
    icon_path: Option<PathBuf>,
) -> Result<(), String> {
    let subkey = HSTRING::from(format!(r"SOFTWARE\Classes\AppUserModelId\{app_id}"));
    let mut key = HKEY::default();

    let result = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &subkey,
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
    };
    check_win32(result, "create notification app identity registry key")?;

    let set_result = (|| {
        set_string_value(key, "DisplayName", app_name)?;
        set_string_value(key, "IconBackgroundColor", "0")?;
        if let Some(icon_path) = icon_path {
            if let Some(icon_uri) = icon_path.to_str() {
                set_string_value(key, "IconUri", icon_uri)?;
            }
        }
        Ok(())
    })();

    let close_result = unsafe { RegCloseKey(key) };
    check_win32(close_result, "close notification app identity registry key")?;
    set_result
}

fn set_string_value(key: HKEY, name: &str, value: &str) -> Result<(), String> {
    let value_name = HSTRING::from(name);
    let bytes = reg_sz_bytes(value);
    let result = unsafe { RegSetValueExW(key, &value_name, None, REG_SZ, Some(&bytes)) };
    check_win32(result, "set notification app identity registry value")
}

fn reg_sz_bytes(value: &str) -> Vec<u8> {
    value
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn check_win32(result: WIN32_ERROR, action: &str) -> Result<(), String> {
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("{action} failed with code {}", result.0))
    }
}
