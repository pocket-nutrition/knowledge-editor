use serde::Serialize;
use tauri::State;

use crate::error::AppResult;

use super::AppState;

const DEFAULT_COMMUNITY_URL: &str = "http://localhost:8001";

#[derive(Debug, Serialize)]
pub struct ReloadResponse {
    pub ok: bool,
    pub detail: Option<String>,
}

#[tauri::command]
pub async fn trigger_community_reload(state: State<'_, AppState>) -> AppResult<ReloadResponse> {
    let base = state
        .repo
        .config
        .community_url
        .clone()
        .unwrap_or_else(|| DEFAULT_COMMUNITY_URL.to_string());
    let token = state.repo.config.community_reload_token.clone();

    let url = format!("{}/admin/reload", base.trim_end_matches('/'));
    let mut req = reqwest::Client::new().post(&url);
    if let Some(t) = token {
        req = req.header("X-Reload-Token", t);
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Ok(ReloadResponse {
                ok: status.is_success(),
                detail: if status.is_success() {
                    None
                } else {
                    Some(format!("{status}: {}", truncate(&body, 256)))
                },
            })
        }
        Err(e) => Ok(ReloadResponse {
            ok: false,
            detail: Some(e.to_string()),
        }),
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}
