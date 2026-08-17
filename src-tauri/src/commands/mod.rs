pub mod dashboard;
pub mod git;
pub mod ingredients;
pub mod mappings;
pub mod reload;

use parking_lot::RwLock;
use std::sync::Arc;

use crate::indexer::Index;
use crate::repo::Repo;
use crate::schema::Schemas;

/// Tauri-managed state injected into every command via `tauri::State<AppState>`.
pub struct AppState {
    pub repo: Repo,
    pub schemas: Schemas,
    pub index: Arc<RwLock<Index>>,
}
