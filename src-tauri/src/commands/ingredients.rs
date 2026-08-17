use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::model::{
    CookingRelationship, FoodGroup, Ingredient, SourceMapping, StateMetadata, TypicalState,
};
use crate::yaml_io::write_yaml_atomic;

use super::AppState;

/// Per-ingredient signals used to compute the list-view completeness score.
#[derive(Debug, Default, Clone, Serialize)]
pub struct CompletenessScore {
    /// 0..=100 weighted blend of the three dimensions below.
    pub overall: u8,
    /// Metadata richness (name_fr, typical_state, can_eat_*, aliases).
    pub identity: u8,
    /// Source mapping coverage / validation status.
    pub sources: u8,
    /// Cooking relationships when the ingredient has cooked states.
    pub cooking: u8,
    pub mapping_count: u32,
    pub linked_mapping_count: u32,
    pub validated_mapping_count: u32,
    pub relationship_count: u32,
    /// True if the ingredient has a non-raw state that needs cooking data.
    pub needs_cooking: bool,
}

/// Tri-state orphan filter. `Hide` is the default — most curation flows
/// don't need the backlog mixed in. `Only` surfaces the backlog for triage;
/// `All` shows everything regardless of mapping coverage.
#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrphansFilter {
    #[default]
    Hide,
    Only,
    All,
}

/// List sort order. Applied in the backend across the whole filtered set
/// (before pagination) so the order is correct across pages, not just within
/// the visible page.
#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortKey {
    #[default]
    Name,
    ScoreAsc,
    ScoreDesc,
    /// Most un-validated mappings first (largest curation backlog first).
    UnvalidatedDesc,
    UnvalidatedAsc,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IngredientsListFilter {
    pub search: Option<String>,
    pub food_group: Option<FoodGroup>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
    pub orphans_filter: Option<OrphansFilter>,
    pub sort: Option<SortKey>,
}

#[derive(Debug, Serialize)]
pub struct IngredientSummary {
    pub id: String,
    pub name_en: String,
    pub name_fr: Option<String>,
    pub food_group: FoodGroup,
    pub states: Vec<String>,
    pub display_unit: Option<String>,
    pub completeness: CompletenessScore,
}

/// Per-ingredient mapping aggregates collected in a single sweep of the index.
#[derive(Default, Clone, Copy)]
struct MappingTally {
    total: u32,
    linked: u32,
    validated: u32,
}

fn tally_mappings<'a>(
    mappings: impl Iterator<Item = &'a SourceMapping>,
) -> HashMap<String, MappingTally> {
    let mut out: HashMap<String, MappingTally> = HashMap::new();
    for m in mappings {
        let Some(id) = m.canonical_ingredient_id.as_deref() else {
            continue;
        };
        if id.is_empty() || id == SourceMapping::COMPOSITE_SENTINEL {
            continue;
        }
        let entry = out.entry(id.to_string()).or_default();
        entry.total += 1;
        if m.is_linked() {
            entry.linked += 1;
        }
        let is_validated = m.match_method.as_deref() == Some("manual") && m.confidence == Some(1.0);
        if is_validated {
            entry.validated += 1;
        }
    }
    out
}

fn tally_relationships<'a>(
    relationships: impl Iterator<Item = &'a CookingRelationship>,
) -> HashMap<String, u32> {
    let mut out: HashMap<String, u32> = HashMap::new();
    for r in relationships {
        *out.entry(r.raw_id.clone()).or_insert(0) += 1;
    }
    out
}

fn score_identity(i: &Ingredient) -> u8 {
    let mut s = 0u32;
    if i.name_fr.as_deref().map_or(false, |n| !n.is_empty()) {
        s += 40;
    }
    if i.typical_state.is_some() {
        s += 30;
    }
    if i.can_eat_raw.is_some() {
        s += 15;
    }
    if i.can_eat_cooked.is_some() {
        s += 15;
    }
    s.min(100) as u8
}

fn score_sources(t: MappingTally) -> u8 {
    if t.validated >= 1 {
        100
    } else if t.linked >= 2 {
        70
    } else if t.linked == 1 {
        45
    } else if t.total >= 1 {
        20
    } else {
        0
    }
}

fn needs_cooking(i: &Ingredient) -> bool {
    i.states.keys().any(|k| k != "raw" && !k.is_empty())
}

fn score_cooking(needs: bool, relationship_count: u32) -> u8 {
    if !needs {
        return 100;
    }
    match relationship_count {
        0 => 0,
        1 => 60,
        _ => 100,
    }
}

fn build_completeness(
    i: &Ingredient,
    tally: MappingTally,
    relationship_count: u32,
) -> CompletenessScore {
    let identity = score_identity(i);
    let sources = score_sources(tally);
    let needs = needs_cooking(i);
    let cooking = score_cooking(needs, relationship_count);
    // Equal-weight blend keeps the score easy to reason about; tweak here only.
    let overall = ((identity as u32 + sources as u32 + cooking as u32) / 3) as u8;
    CompletenessScore {
        overall,
        identity,
        sources,
        cooking,
        mapping_count: tally.total,
        linked_mapping_count: tally.linked,
        validated_mapping_count: tally.validated,
        relationship_count,
        needs_cooking: needs,
    }
}

#[derive(Debug, Serialize)]
pub struct Facets {
    pub food_group: std::collections::BTreeMap<String, usize>,
}

#[derive(Debug, Serialize)]
pub struct IngredientsListResponse {
    pub items: Vec<IngredientSummary>,
    pub total: usize,
    pub facets: Facets,
}

#[tauri::command]
pub fn list_ingredients(
    state: State<'_, AppState>,
    filter: IngredientsListFilter,
) -> AppResult<IngredientsListResponse> {
    let idx = state.index.read();
    let query = filter.search.unwrap_or_default().trim().to_lowercase();
    let target_group = filter.food_group;
    let orphans_filter = filter.orphans_filter.unwrap_or_default();
    let sort = filter.sort.unwrap_or_default();

    // Mapping tallies feed both the orphan filter and the per-row completeness
    // score below, so build them once and reuse.
    let mapping_tallies = tally_mappings(idx.mappings.values());

    let filtered: Vec<&Ingredient> = idx
        .ingredients
        .values()
        .filter(|i| match &target_group {
            Some(g) => &i.food_group == g,
            None => true,
        })
        .filter(|i| {
            let is_orphan = mapping_tallies
                .get(&i.id)
                .map(|t| t.total == 0)
                .unwrap_or(true);
            match orphans_filter {
                OrphansFilter::All => true,
                OrphansFilter::Only => is_orphan,
                OrphansFilter::Hide => !is_orphan,
            }
        })
        .filter(|i| {
            if query.is_empty() {
                return true;
            }
            let hay = [
                Some(i.name_en.as_str()),
                i.name_fr.as_deref(),
                Some(i.id.as_str()),
            ];
            let direct = hay
                .into_iter()
                .flatten()
                .any(|s| s.to_lowercase().contains(&query));
            if direct {
                return true;
            }
            let mut aliases = i.aliases_en.iter().chain(i.aliases_fr.iter()).flatten();
            aliases.any(|a| a.to_lowercase().contains(&query))
        })
        .collect();

    // Facets computed over the unfiltered population so the dropdown stays
    // stable regardless of search/group state.
    let mut facets_map = std::collections::BTreeMap::new();
    for ing in idx.ingredients.values() {
        *facets_map
            .entry(food_group_str(&ing.food_group))
            .or_insert(0usize) += 1;
    }

    let offset = filter.offset.unwrap_or(0);
    let limit = filter.limit.unwrap_or(50).clamp(1, 500);

    // Relationship counts feed the completeness score. Build them once so the
    // score (and any score-based sort) is computed for the whole filtered set
    // before pagination.
    let relationship_counts = tally_relationships(idx.relationships.values());

    // Materialise (ingredient, completeness) for the whole filtered set so the
    // sort below orders across all pages, not just the visible window.
    let mut scored: Vec<(&Ingredient, CompletenessScore)> = filtered
        .into_iter()
        .map(|i| {
            let tally = mapping_tallies.get(&i.id).copied().unwrap_or_default();
            let rels = relationship_counts.get(&i.id).copied().unwrap_or(0);
            (i, build_completeness(i, tally, rels))
        })
        .collect();

    let by_name =
        |a: &Ingredient, b: &Ingredient| a.name_en.to_lowercase().cmp(&b.name_en.to_lowercase());
    let unvalidated =
        |c: &CompletenessScore| c.mapping_count.saturating_sub(c.validated_mapping_count);
    match sort {
        SortKey::Name => scored.sort_by(|(a, _), (b, _)| by_name(a, b)),
        SortKey::ScoreAsc => scored
            .sort_by(|(a, ca), (b, cb)| ca.overall.cmp(&cb.overall).then_with(|| by_name(a, b))),
        SortKey::ScoreDesc => scored
            .sort_by(|(a, ca), (b, cb)| cb.overall.cmp(&ca.overall).then_with(|| by_name(a, b))),
        SortKey::UnvalidatedDesc => scored.sort_by(|(a, ca), (b, cb)| {
            unvalidated(cb)
                .cmp(&unvalidated(ca))
                .then_with(|| by_name(a, b))
        }),
        SortKey::UnvalidatedAsc => scored.sort_by(|(a, ca), (b, cb)| {
            unvalidated(ca)
                .cmp(&unvalidated(cb))
                .then_with(|| by_name(a, b))
        }),
    }

    let total = scored.len();

    let items: Vec<IngredientSummary> = scored
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(i, completeness)| IngredientSummary {
            id: i.id.clone(),
            name_en: i.name_en.clone(),
            name_fr: i.name_fr.clone(),
            food_group: i.food_group.clone(),
            states: i.states.keys().cloned().collect(),
            display_unit: i.display_unit.clone(),
            completeness,
        })
        .collect();

    Ok(IngredientsListResponse {
        items,
        total,
        facets: Facets {
            food_group: facets_map,
        },
    })
}

#[derive(Debug, Serialize)]
pub struct IngredientDetailResponse {
    pub ingredient: Ingredient,
    pub mappings: Vec<SourceMapping>,
    pub relationships: Vec<CookingRelationship>,
}

#[tauri::command]
pub fn get_ingredient(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<IngredientDetailResponse> {
    let idx = state.index.read();
    let ingredient = idx
        .ingredients
        .get(&id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("ingredient {id}")))?;

    let mappings: Vec<SourceMapping> = idx
        .mappings
        .values()
        .filter(|m| m.canonical_ingredient_id.as_deref() == Some(id.as_str()))
        .cloned()
        .collect();

    let relationships: Vec<CookingRelationship> = idx
        .relationships
        .values()
        .filter(|r| r.raw_id == id)
        .cloned()
        .collect();

    Ok(IngredientDetailResponse {
        ingredient,
        mappings,
        relationships,
    })
}

#[tauri::command]
pub fn save_ingredient(
    state: State<'_, AppState>,
    ingredient: Ingredient,
) -> AppResult<Ingredient> {
    let path = state.repo.ingredient_path(&ingredient.id);
    let as_value = serde_yaml::to_value(&ingredient)?;
    let as_json: serde_json::Value = serde_yaml::from_value(as_value)?;
    state.schemas.validate_ingredient(&path, &as_json)?;
    write_yaml_atomic(&path, &ingredient)?;

    let mut idx = state.index.write();
    idx.ingredients
        .insert(ingredient.id.clone(), ingredient.clone());
    Ok(ingredient)
}

/// Creates a new ingredient that copies every field of `source_id` except `id`
/// and `name_en`. Source mappings are NOT copied (they live in separate YAMLs
/// keyed by canonical_ingredient_id), so the clone starts with zero mappings —
/// the curator attaches new ones to give it different nutrition.
///
/// Unlike `save_ingredient`, this refuses to overwrite: an existing `new_id`
/// is rejected so a clone can never clobber another ingredient.
#[tauri::command]
pub fn clone_ingredient(
    state: State<'_, AppState>,
    source_id: String,
    new_id: String,
    new_name_en: String,
) -> AppResult<Ingredient> {
    let new_id = new_id.trim();
    let new_name_en = new_name_en.trim();
    if new_id.is_empty() {
        return Err(AppError::InvalidInput("new ingredient id is empty".into()));
    }
    if new_name_en.is_empty() {
        return Err(AppError::InvalidInput(
            "new ingredient name is empty".into(),
        ));
    }

    let mut clone = {
        let idx = state.index.read();
        if idx.ingredients.contains_key(new_id) {
            return Err(AppError::InvalidInput(format!(
                "ingredient {new_id} already exists"
            )));
        }
        idx.ingredients
            .get(source_id.trim())
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("ingredient {source_id}")))?
    };

    clone.id = new_id.to_string();
    clone.name_en = new_name_en.to_string();

    // Same validate -> atomic write -> index path as save_ingredient. The
    // id pattern (^[a-z0-9_]{1,128}$) and name minLength are enforced here.
    let path = state.repo.ingredient_path(&clone.id);
    let as_value = serde_yaml::to_value(&clone)?;
    let as_json: serde_json::Value = serde_yaml::from_value(as_value)?;
    state.schemas.validate_ingredient(&path, &as_json)?;
    write_yaml_atomic(&path, &clone)?;

    let mut idx = state.index.write();
    idx.ingredients.insert(clone.id.clone(), clone.clone());
    Ok(clone)
}

#[tauri::command]
pub fn set_ingredient_food_group(
    state: State<'_, AppState>,
    id: String,
    food_group: FoodGroup,
) -> AppResult<Ingredient> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("ingredient id is empty".into()));
    }

    // Read-modify-write through the same atomic path + schema check used by
    // save_ingredient so a bulk reclassification can never produce an invalid
    // YAML on disk.
    let mut updated = {
        let idx = state.index.read();
        idx.ingredients
            .get(trimmed)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("ingredient {trimmed}")))?
    };
    if updated.food_group == food_group {
        return Ok(updated);
    }
    updated.food_group = food_group;

    let path = state.repo.ingredient_path(&updated.id);
    let as_value = serde_yaml::to_value(&updated)?;
    let as_json: serde_json::Value = serde_yaml::from_value(as_value)?;
    state.schemas.validate_ingredient(&path, &as_json)?;
    write_yaml_atomic(&path, &updated)?;

    let mut idx = state.index.write();
    idx.ingredients.insert(updated.id.clone(), updated.clone());
    Ok(updated)
}

#[tauri::command]
pub fn set_ingredient_display_unit(
    state: State<'_, AppState>,
    id: String,
    display_unit: Option<String>,
) -> AppResult<Ingredient> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("ingredient id is empty".into()));
    }
    // The YAML schema only allows "g" or "ml" (or absent); guard here so a
    // bulk action cannot land an unknown unit on disk.
    let normalised: Option<String> = match display_unit.as_deref() {
        None => None,
        Some(s) if s.trim().is_empty() => None,
        Some("g") => Some("g".into()),
        Some("ml") => Some("ml".into()),
        Some(other) => {
            return Err(AppError::InvalidInput(format!(
                "display_unit must be \"g\", \"ml\", or null (got {other:?})"
            )));
        }
    };

    let mut updated = {
        let idx = state.index.read();
        idx.ingredients
            .get(trimmed)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("ingredient {trimmed}")))?
    };
    if updated.display_unit == normalised {
        return Ok(updated);
    }
    updated.display_unit = normalised;

    let path = state.repo.ingredient_path(&updated.id);
    let as_value = serde_yaml::to_value(&updated)?;
    let as_json: serde_json::Value = serde_yaml::from_value(as_value)?;
    state.schemas.validate_ingredient(&path, &as_json)?;
    write_yaml_atomic(&path, &updated)?;

    let mut idx = state.index.write();
    idx.ingredients.insert(updated.id.clone(), updated.clone());
    Ok(updated)
}

/// Shared read-modify-write helper for the granular bulk-edit setters.
///
/// The mutator returns `true` if anything actually changed; when nothing
/// changes we skip the schema-validate + atomic-write step entirely so a
/// no-op bulk apply does not rewrite YAML on disk.
fn rmw_ingredient<F>(state: &State<'_, AppState>, id: &str, mutator: F) -> AppResult<Ingredient>
where
    F: FnOnce(&mut Ingredient) -> AppResult<bool>,
{
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("ingredient id is empty".into()));
    }
    let mut updated = {
        let idx = state.index.read();
        idx.ingredients
            .get(trimmed)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("ingredient {trimmed}")))?
    };
    let changed = mutator(&mut updated)?;
    if !changed {
        return Ok(updated);
    }
    let path = state.repo.ingredient_path(&updated.id);
    let as_value = serde_yaml::to_value(&updated)?;
    let as_json: serde_json::Value = serde_yaml::from_value(as_value)?;
    state.schemas.validate_ingredient(&path, &as_json)?;
    write_yaml_atomic(&path, &updated)?;
    let mut idx = state.index.write();
    idx.ingredients.insert(updated.id.clone(), updated.clone());
    Ok(updated)
}

/// `typical_state = None` clears the field (skip_serializing_if drops it from
/// YAML); `Some(state)` overwrites. Schema validation will reject any value
/// that isn't in the allowed enum.
#[tauri::command]
pub fn set_ingredient_typical_state(
    state: State<'_, AppState>,
    id: String,
    typical_state: Option<TypicalState>,
) -> AppResult<Ingredient> {
    rmw_ingredient(&state, &id, |ing| {
        if ing.typical_state == typical_state {
            return Ok(false);
        }
        ing.typical_state = typical_state;
        Ok(true)
    })
}

/// `value = None` clears `can_eat_raw` (drops the key from YAML); `Some(b)`
/// sets it explicitly. The curator uses "leave alone" by not calling at all.
#[tauri::command]
pub fn set_ingredient_can_eat_raw(
    state: State<'_, AppState>,
    id: String,
    value: Option<bool>,
) -> AppResult<Ingredient> {
    rmw_ingredient(&state, &id, |ing| {
        if ing.can_eat_raw == value {
            return Ok(false);
        }
        ing.can_eat_raw = value;
        Ok(true)
    })
}

/// See `set_ingredient_can_eat_raw` — same semantics for the cooked side.
#[tauri::command]
pub fn set_ingredient_can_eat_cooked(
    state: State<'_, AppState>,
    id: String,
    value: Option<bool>,
) -> AppResult<Ingredient> {
    rmw_ingredient(&state, &id, |ing| {
        if ing.can_eat_cooked == value {
            return Ok(false);
        }
        ing.can_eat_cooked = value;
        Ok(true)
    })
}

/// Set or clear the `portion_g` of a specific state. `portion_g = None`
/// drops the field from that state's metadata. If the state key does not
/// exist yet on the ingredient it is created with the metadata empty (the
/// schema requires the state key to be one of raw/cooked/processed — that
/// is enforced at the write-time schema validation, not here).
#[tauri::command]
pub fn set_ingredient_state_portion(
    state: State<'_, AppState>,
    id: String,
    state_key: String,
    portion_g: Option<f64>,
) -> AppResult<Ingredient> {
    let key = state_key.trim().to_string();
    if key.is_empty() {
        return Err(AppError::InvalidInput("state_key is empty".into()));
    }
    rmw_ingredient(&state, &id, move |ing| {
        let entry = ing
            .states
            .entry(key.clone())
            .or_insert_with(StateMetadata::default);
        if entry.portion_g == portion_g {
            return Ok(false);
        }
        entry.portion_g = portion_g;
        Ok(true)
    })
}

/// Set or clear the `density_g_ml` of a specific state. Same semantics as
/// `set_ingredient_state_portion`.
#[tauri::command]
pub fn set_ingredient_state_density(
    state: State<'_, AppState>,
    id: String,
    state_key: String,
    density_g_ml: Option<f64>,
) -> AppResult<Ingredient> {
    let key = state_key.trim().to_string();
    if key.is_empty() {
        return Err(AppError::InvalidInput("state_key is empty".into()));
    }
    rmw_ingredient(&state, &id, move |ing| {
        let entry = ing
            .states
            .entry(key.clone())
            .or_insert_with(StateMetadata::default);
        if entry.density_g_ml == density_g_ml {
            return Ok(false);
        }
        entry.density_g_ml = density_g_ml;
        Ok(true)
    })
}

#[tauri::command]
pub fn delete_ingredient(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("ingredient id is empty".into()));
    }

    // Safety: only orphans may be deleted. Even though the UI hides this action
    // behind the orphans-only filter, the backend re-checks against the live
    // index so a stale frontend cannot drop a YAML that is still referenced.
    {
        let idx = state.index.read();
        if !idx.ingredients.contains_key(trimmed) {
            return Err(AppError::NotFound(format!("ingredient {trimmed}")));
        }
        let referenced = idx
            .mappings
            .values()
            .any(|m| m.canonical_ingredient_id.as_deref() == Some(trimmed));
        if referenced {
            return Err(AppError::InvalidInput(format!(
                "ingredient {trimmed} is referenced by source mappings — unlink them first"
            )));
        }
    }

    let path = state.repo.ingredient_path(trimmed);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        // The in-memory entry exists but the file is already gone — keep going
        // so the index ends in a consistent state.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!(path = %path.display(), "yaml already missing on disk");
        }
        Err(e) => return Err(AppError::Io(e)),
    }

    let mut idx = state.index.write();
    idx.ingredients.remove(trimmed);
    Ok(())
}

fn food_group_str(g: &FoodGroup) -> String {
    serde_json::to_value(g)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}
