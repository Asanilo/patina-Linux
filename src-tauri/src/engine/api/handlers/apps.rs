use crate::data::sqlite_pool;
use crate::engine::api::types::{
    ApiError, ApiResponse, AppEntry, AppsResponse, ClassifyRequest, ExcludeRequest, RenameRequest,
    RouteResponse,
};
use sqlx::Row;

pub async fn get_apps(app: &tauri::AppHandle) -> RouteResponse {
    let pool = match sqlite_pool::wait_for_sqlite_pool(app).await {
        Ok(p) => p,
        Err(e) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&e)).unwrap_or_default(),
            };
        }
    };

    // Get distinct exe_names from sessions
    let rows = match sqlx::query("SELECT DISTINCT exe_name FROM sessions ORDER BY exe_name")
        .fetch_all(&pool)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&e.to_string())).unwrap_or_default(),
            };
        }
    };

    // Load app overrides (display_name, category, excluded) from settings
    let override_rows = sqlx::query(
        "SELECT key, value FROM settings WHERE key LIKE '__app_override::%' OR key LIKE '__app_category::%' OR key LIKE '__app_excluded::%'",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();

    let mut display_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut categories: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut excluded_set: std::collections::HashSet<String> = std::collections::HashSet::new();

    for row in &override_rows {
        let key: String = row.try_get("key").unwrap_or_default();
        let value: String = row.try_get("value").unwrap_or_default();

        if let Some(exe) = key.strip_prefix("__app_override::") {
            if let Ok(override_data) = serde_json::from_str::<serde_json::Value>(&value) {
                if let Some(name) = override_data.get("display_name").and_then(|v| v.as_str()) {
                    display_names.insert(exe.to_string(), name.to_string());
                }
            }
        } else if let Some(exe) = key.strip_prefix("__app_category::") {
            categories.insert(exe.to_string(), value);
        } else if let Some(exe) = key.strip_prefix("__app_excluded::") {
            if value == "1" || value == "true" {
                excluded_set.insert(exe.to_string());
            }
        }
    }

    let apps: Vec<AppEntry> = rows
        .iter()
        .map(|row| {
            let exe_name: String = row.try_get("exe_name").unwrap_or_default();
            let display_name = display_names
                .get(&exe_name)
                .cloned()
                .unwrap_or_else(|| exe_name.clone());
            let category = categories.get(&exe_name).cloned();
            let excluded = excluded_set.contains(&exe_name);

            AppEntry {
                exe_name,
                display_name,
                category,
                excluded,
            }
        })
        .collect();

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: AppsResponse { apps },
        })
        .unwrap_or_default(),
    }
}

pub async fn handle_app_action(app: &tauri::AppHandle, path: &str, body: &[u8]) -> RouteResponse {
    // Path: /api/v1/apps/{exe_name}/{action}
    let remainder = path.strip_prefix("/api/v1/apps/").unwrap_or("");
    let parts: Vec<&str> = remainder.splitn(2, '/').collect();
    if parts.len() < 2 {
        return RouteResponse {
            status: 400,
            body: serde_json::to_value(ApiError::bad_request("missing action")).unwrap_or_default(),
        };
    }

    let exe_name = parts[0];
    let action = parts[1];

    let pool = match sqlite_pool::wait_for_sqlite_pool(app).await {
        Ok(p) => p,
        Err(e) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&e)).unwrap_or_default(),
            };
        }
    };

    match action {
        "classify" => {
            let req: ClassifyRequest = match serde_json::from_slice(body) {
                Ok(r) => r,
                Err(_) => {
                    return RouteResponse {
                        status: 400,
                        body: serde_json::to_value(ApiError::bad_request("invalid JSON body"))
                            .unwrap_or_default(),
                    };
                }
            };
            let key = format!("__app_category::{exe_name}");
            if let Err(e) = crate::data::repositories::tracker_settings::save_setting_value(
                &pool,
                &key,
                &req.category,
            )
            .await
            {
                return RouteResponse {
                    status: 500,
                    body: serde_json::to_value(ApiError::internal(&e.to_string()))
                        .unwrap_or_default(),
                };
            }
            RouteResponse {
                status: 200,
                body: serde_json::to_value(ApiResponse {
                    data: serde_json::json!({"ok": true}),
                })
                .unwrap_or_default(),
            }
        }
        "rename" => {
            let req: RenameRequest = match serde_json::from_slice(body) {
                Ok(r) => r,
                Err(_) => {
                    return RouteResponse {
                        status: 400,
                        body: serde_json::to_value(ApiError::bad_request("invalid JSON body"))
                            .unwrap_or_default(),
                    };
                }
            };
            let key = format!("__app_override::{exe_name}");
            let value = serde_json::json!({"display_name": req.display_name}).to_string();
            if let Err(e) =
                crate::data::repositories::tracker_settings::save_setting_value(&pool, &key, &value)
                    .await
            {
                return RouteResponse {
                    status: 500,
                    body: serde_json::to_value(ApiError::internal(&e.to_string()))
                        .unwrap_or_default(),
                };
            }
            RouteResponse {
                status: 200,
                body: serde_json::to_value(ApiResponse {
                    data: serde_json::json!({"ok": true}),
                })
                .unwrap_or_default(),
            }
        }
        "exclude" => {
            let req: ExcludeRequest = match serde_json::from_slice(body) {
                Ok(r) => r,
                Err(_) => {
                    return RouteResponse {
                        status: 400,
                        body: serde_json::to_value(ApiError::bad_request("invalid JSON body"))
                            .unwrap_or_default(),
                    };
                }
            };
            let key = format!("__app_excluded::{exe_name}");
            let value = if req.excluded { "1" } else { "0" };
            if let Err(e) =
                crate::data::repositories::tracker_settings::save_setting_value(&pool, &key, value)
                    .await
            {
                return RouteResponse {
                    status: 500,
                    body: serde_json::to_value(ApiError::internal(&e.to_string()))
                        .unwrap_or_default(),
                };
            }
            RouteResponse {
                status: 200,
                body: serde_json::to_value(ApiResponse {
                    data: serde_json::json!({"ok": true}),
                })
                .unwrap_or_default(),
            }
        }
        _ => RouteResponse {
            status: 404,
            body: serde_json::to_value(ApiError::not_found("unknown app action"))
                .unwrap_or_default(),
        },
    }
}
