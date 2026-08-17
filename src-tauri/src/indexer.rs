//! In-memory index of the entire pocket-nutrition/knowledge YAML repo.
//!
//! Scale envelope (current corpus): ~3 500 ingredients, ~20 000 source mappings,
//! ~500 cooking relationships, ~50 MB total YAML. We materialise everything
//! into memory at startup so the UI can drive list / search / facet operations
//! without re-walking the filesystem on every keystroke.
//!
//! Concurrency: the index is shared via `Arc<RwLock<Index>>`. Read paths
//! (list / search / get) take a read lock. Write paths (save_ingredient,
//! link_mapping, …) take a write lock for the duration of the file write +
//! in-memory update.

use std::collections::HashMap;
use std::path::Path;

use walkdir::WalkDir;

use crate::error::AppResult;
use crate::model::{CookingRelationship, Ingredient, Source, SourceMapping};
use crate::repo::Repo;
use crate::yaml_io::read_yaml;

#[derive(Default)]
pub struct Index {
    pub ingredients: HashMap<String, Ingredient>,
    /// (Source, source_id) -> SourceMapping
    pub mappings: HashMap<(Source, String), SourceMapping>,
    /// (raw_id, CookingMethod-as-str) -> CookingRelationship
    pub relationships: HashMap<(String, String), CookingRelationship>,
}

impl Index {
    pub fn build(repo: &Repo) -> AppResult<Self> {
        let mut idx = Index::default();
        load_ingredients(&mut idx, &repo.ingredients_dir());
        load_mappings(&mut idx, &repo.mappings_dir());
        load_relationships(&mut idx, &repo.relationships_dir());
        Ok(idx)
    }
}

fn load_ingredients(idx: &mut Index, dir: &Path) {
    if !dir.is_dir() {
        return;
    }
    for entry in WalkDir::new(dir)
        .max_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| is_yaml(e.path()))
    {
        match read_yaml::<Ingredient>(entry.path()) {
            Ok(i) => {
                idx.ingredients.insert(i.id.clone(), i);
            }
            Err(e) => {
                tracing::warn!("skip {}: {e}", entry.path().display());
            }
        }
    }
}

fn load_mappings(idx: &mut Index, dir: &Path) {
    if !dir.is_dir() {
        return;
    }
    for entry in WalkDir::new(dir)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| is_yaml(e.path()))
    {
        match read_yaml::<SourceMapping>(entry.path()) {
            Ok(m) => {
                idx.mappings.insert((m.source, m.source_id.clone()), m);
            }
            Err(e) => {
                tracing::warn!("skip {}: {e}", entry.path().display());
            }
        }
    }
}

fn load_relationships(idx: &mut Index, dir: &Path) {
    if !dir.is_dir() {
        return;
    }
    for entry in WalkDir::new(dir)
        .max_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| is_yaml(e.path()))
    {
        match read_yaml::<CookingRelationship>(entry.path()) {
            Ok(r) => {
                let key = (r.raw_id.clone(), r.cooking_method.as_str().to_string());
                idx.relationships.insert(key, r);
            }
            Err(e) => {
                tracing::warn!("skip {}: {e}", entry.path().display());
            }
        }
    }
}

fn is_yaml(p: &Path) -> bool {
    p.is_file()
        && p.extension()
            .map_or(false, |ext| ext == "yaml" || ext == "yml")
}
