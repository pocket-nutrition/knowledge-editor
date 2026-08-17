//! Atomic, lossy-stable YAML read/write helpers.
//!
//! Stability properties:
//! - keys serialised in serde declaration order (Ingredient/SourceMapping/CookingRelationship)
//! - `Option::None` fields omitted (skip_serializing_if = "Option::is_none")
//! - block-style YAML (serde_yaml default)
//!
//! This minimises noisy diffs vs what `pocket-nutrition/community` produces via PyYAML
//! with `sort_keys=False`. It does *not* preserve original key order on
//! round-trip; we accept that trade-off because Community itself doesn't.

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;
use tempfile::NamedTempFile;

use crate::error::{AppError, AppResult};

pub fn read_yaml<T: DeserializeOwned>(path: &Path) -> AppResult<T> {
    let content = fs::read_to_string(path)?;
    serde_yaml::from_str(&content).map_err(|e| AppError::YamlParse {
        path: path.display().to_string(),
        source: e,
    })
}

pub fn write_yaml_atomic<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::InvalidInput(format!("no parent dir for {}", path.display())))?;
    fs::create_dir_all(parent)?;

    let yaml = serde_yaml::to_string(value)?;
    // serde_yaml v0.9 adds a `---\n` document marker we don't want, since
    // existing files in pocket-nutrition/knowledge omit it.
    let trimmed = yaml.strip_prefix("---\n").map(String::from).unwrap_or(yaml);

    let mut tmp = NamedTempFile::new_in(parent)?;
    tmp.write_all(trimmed.as_bytes())?;
    tmp.flush()?;
    tmp.persist(path)
        .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
    Ok(())
}
