//! Read-only git surface — never writes. The user commits manually.

use serde::Serialize;
use std::process::Command;
use tauri::State;

use crate::error::{AppError, AppResult};

use super::AppState;

#[derive(Debug, Serialize, Default)]
pub struct GitStatus {
    pub modified: Vec<String>,
    pub untracked: Vec<String>,
}

#[tauri::command]
pub fn git_status(state: State<'_, AppState>) -> AppResult<GitStatus> {
    let out = Command::new("git")
        .arg("-C")
        .arg(&state.repo.root)
        .args(["status", "--porcelain"])
        .output()
        .map_err(|e| AppError::Git(format!("failed to run git: {e}")))?;

    if !out.status.success() {
        return Err(AppError::Git(format!(
            "git status exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut status = GitStatus::default();
    for line in stdout.lines() {
        if line.len() < 3 {
            continue;
        }
        let code = &line[..2];
        let path = line[3..].to_string();
        if code.starts_with("??") {
            status.untracked.push(path);
        } else {
            status.modified.push(path);
        }
    }
    Ok(status)
}
