use serde::Serialize;
use std::collections::BTreeMap;
use tauri::State;

use crate::error::AppResult;
use crate::model::{FoodGroup, Source};

use super::AppState;

#[derive(Debug, Serialize, Default)]
pub struct SourceStats {
    pub total: usize,
    pub linked: usize,
    pub composite: usize,
    pub unlinked: usize,
}

#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub total_ingredients: usize,
    pub by_food_group: BTreeMap<String, usize>,
    pub total_mappings: usize,
    pub mappings_by_source: BTreeMap<String, SourceStats>,
    pub total_relationships: usize,
}

#[tauri::command]
pub fn dashboard_stats(state: State<'_, AppState>) -> AppResult<DashboardStats> {
    let idx = state.index.read();

    let mut by_group: BTreeMap<String, usize> = BTreeMap::new();
    for ing in idx.ingredients.values() {
        *by_group.entry(food_group_str(&ing.food_group)).or_insert(0) += 1;
    }

    let mut by_src: BTreeMap<String, SourceStats> = BTreeMap::new();
    for s in Source::all() {
        by_src.insert(s.as_dir().to_string(), SourceStats::default());
    }
    for m in idx.mappings.values() {
        let entry = by_src.entry(m.source.as_dir().to_string()).or_default();
        entry.total += 1;
        if m.is_linked() {
            entry.linked += 1;
        } else if m.is_composite() {
            entry.composite += 1;
        } else {
            entry.unlinked += 1;
        }
    }

    Ok(DashboardStats {
        total_ingredients: idx.ingredients.len(),
        by_food_group: by_group,
        total_mappings: idx.mappings.len(),
        mappings_by_source: by_src,
        total_relationships: idx.relationships.len(),
    })
}

fn food_group_str(g: &FoodGroup) -> String {
    serde_json::to_value(g)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}
