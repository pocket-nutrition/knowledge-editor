mod commands;
pub mod error;
pub mod indexer;
pub mod model;
pub mod repo;
pub mod schema;
pub mod yaml_io;

use std::sync::Arc;

use parking_lot::RwLock;

use commands::AppState;
use indexer::Index;
use repo::Repo;
use schema::Schemas;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .compact()
        .init();

    let state = match build_state() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("startup failed: {e}");
            eprintln!("pocket-nutrition-knowledge-editor: startup failed: {e}");
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::dashboard::dashboard_stats,
            commands::ingredients::list_ingredients,
            commands::ingredients::get_ingredient,
            commands::ingredients::save_ingredient,
            commands::ingredients::clone_ingredient,
            commands::ingredients::delete_ingredient,
            commands::ingredients::set_ingredient_food_group,
            commands::ingredients::set_ingredient_display_unit,
            commands::ingredients::set_ingredient_typical_state,
            commands::ingredients::set_ingredient_can_eat_raw,
            commands::ingredients::set_ingredient_can_eat_cooked,
            commands::ingredients::set_ingredient_state_portion,
            commands::ingredients::set_ingredient_state_density,
            commands::mappings::unlink_mapping,
            commands::mappings::mark_mapping_composite,
            commands::mappings::validate_mapping,
            commands::mappings::set_mapping_cooking_method,
            commands::mappings::link_mapping,
            commands::mappings::search_unlinked_mappings,
            commands::git::git_status,
            commands::reload::trigger_community_reload,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn build_state() -> error::AppResult<AppState> {
    let repo = Repo::discover()?;
    tracing::info!(root = %repo.root.display(), "loaded knowledge repo");
    let schemas = Schemas::load(&repo.schemas_dir())?;
    let index = Index::build(&repo)?;
    tracing::info!(
        ingredients = index.ingredients.len(),
        mappings = index.mappings.len(),
        relationships = index.relationships.len(),
        "index built"
    );
    Ok(AppState {
        repo,
        schemas,
        index: Arc::new(RwLock::new(index)),
    })
}
