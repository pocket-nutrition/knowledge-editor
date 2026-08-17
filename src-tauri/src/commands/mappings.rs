//! Per-mapping curation actions surfaced on the ingredient detail page.
//!
//! All three commands mutate a single SourceMapping YAML in place, validate
//! against the JSON schema, then update the in-memory index. The user commits
//! manually via git afterwards.

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::model::{Source, SourceMapping, SourceMappingCookingMethod};
use crate::yaml_io::write_yaml_atomic;

use super::AppState;

/// Clears `canonical_ingredient_id` so the mapping re-enters the unlinked pool.
/// Use when the auto-match picked the wrong canonical ingredient.
#[tauri::command]
pub fn unlink_mapping(
    state: State<'_, AppState>,
    source: Source,
    source_id: String,
) -> AppResult<SourceMapping> {
    update_mapping(&state, source, &source_id, |m| {
        m.canonical_ingredient_id = None;
    })
}

/// Marks a mapping as intentionally un-mappable (composite / multi-ingredient).
#[tauri::command]
pub fn mark_mapping_composite(
    state: State<'_, AppState>,
    source: Source,
    source_id: String,
) -> AppResult<SourceMapping> {
    update_mapping(&state, source, &source_id, |m| {
        m.canonical_ingredient_id = Some(SourceMapping::COMPOSITE_SENTINEL.into());
    })
}

/// Promotes an existing link to human-validated status: `match_method = manual`,
/// `confidence = 1.0`. Refuses if the mapping isn't currently linked to a
/// concrete canonical ingredient (nothing to validate).
#[tauri::command]
pub fn validate_mapping(
    state: State<'_, AppState>,
    source: Source,
    source_id: String,
) -> AppResult<SourceMapping> {
    update_mapping(&state, source, &source_id, |m| {
        m.match_method = Some("manual".into());
        m.confidence = Some(1.0);
    })
    .and_then(|m| {
        if m.is_linked() {
            Ok(m)
        } else {
            Err(AppError::InvalidInput(
                "cannot validate an unlinked or composite mapping".into(),
            ))
        }
    })
}

/// Sets the `cooking_method` on an existing source mapping so it is grouped
/// correctly on the ingredient detail page (e.g. "roasted" instead of "raw").
#[tauri::command]
pub fn set_mapping_cooking_method(
    state: State<'_, AppState>,
    source: Source,
    source_id: String,
    cooking_method: SourceMappingCookingMethod,
) -> AppResult<SourceMapping> {
    update_mapping(&state, source, &source_id, |m| {
        m.cooking_method = Some(cooking_method);
    })
}

/// Links an unlinked mapping to a canonical ingredient and marks it as
/// manually validated (`match_method = "manual"`, `confidence = 1.0`).
#[tauri::command]
pub fn link_mapping(
    state: State<'_, AppState>,
    source: Source,
    source_id: String,
    ingredient_id: String,
) -> AppResult<SourceMapping> {
    update_mapping(&state, source, &source_id, |m| {
        m.canonical_ingredient_id = Some(ingredient_id);
        m.match_method = Some("manual".into());
        m.confidence = Some(1.0);
    })
}

/// Returns up to `limit` (default 20) unlinked mappings whose `label` or
/// `source_id` contains `query` (case-insensitive). Optionally restricted to
/// one `source`. Composite-sentinel entries are excluded.
#[tauri::command]
pub fn search_unlinked_mappings(
    state: State<'_, AppState>,
    source: Option<Source>,
    query: String,
    limit: Option<u32>,
) -> AppResult<Vec<SourceMapping>> {
    let limit = limit.unwrap_or(20) as usize;
    let q = query.to_lowercase();
    let idx = state.index.read();
    let mut results: Vec<SourceMapping> = idx
        .mappings
        .values()
        .filter(|m| m.is_unlinked())
        .filter(|m| source.map_or(true, |s| m.source == s))
        .filter(|m| {
            if q.is_empty() {
                return true;
            }
            let label_match = m
                .label
                .as_deref()
                .map_or(false, |l| l.to_lowercase().contains(&q));
            let id_match = m.source_id.to_lowercase().contains(&q);
            label_match || id_match
        })
        .cloned()
        .collect();

    results.sort_by(|a, b| {
        let la = a.label.as_deref().unwrap_or("");
        let lb = b.label.as_deref().unwrap_or("");
        la.cmp(lb)
    });
    results.truncate(limit);
    Ok(results)
}

fn update_mapping<F: FnOnce(&mut SourceMapping)>(
    state: &State<'_, AppState>,
    source: Source,
    source_id: &str,
    mutate: F,
) -> AppResult<SourceMapping> {
    let key = (source, source_id.to_string());
    let mut updated = {
        let idx = state.index.read();
        idx.mappings
            .get(&key)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("mapping {}#{source_id}", source.as_dir())))?
    };
    mutate(&mut updated);

    let path = state.repo.mapping_path(source.as_dir(), &updated.source_id);
    let as_value = serde_yaml::to_value(&updated)?;
    let as_json: serde_json::Value = serde_yaml::from_value(as_value)?;
    state.schemas.validate_source_mapping(&path, &as_json)?;
    write_yaml_atomic(&path, &updated)?;

    let mut idx = state.index.write();
    idx.mappings.insert(key, updated.clone());
    Ok(updated)
}
