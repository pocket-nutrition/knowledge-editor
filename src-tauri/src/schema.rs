//! JSON-schema validator backed by pocket-nutrition/knowledge/schemas/ingredient.schema.json.
//!
//! Same contract as `pocket-nutrition/knowledge/scripts/validate.py` for ingredients,
//! so the editor can never disagree with the Python validator the user runs
//! before committing.

use std::path::Path;

use jsonschema::Validator;
use serde_json::Value;

use crate::error::{AppError, AppResult};

pub struct Schemas {
    ingredient: Validator,
    source_mapping: Validator,
}

impl Schemas {
    pub fn load(schemas_dir: &Path) -> AppResult<Self> {
        Ok(Self {
            ingredient: load_one(&schemas_dir.join("ingredient.schema.json"))?,
            source_mapping: load_one(&schemas_dir.join("source_mapping.schema.json"))?,
        })
    }

    pub fn validate_ingredient(&self, path: &Path, value: &Value) -> AppResult<()> {
        run(&self.ingredient, path, value)
    }

    pub fn validate_source_mapping(&self, path: &Path, value: &Value) -> AppResult<()> {
        run(&self.source_mapping, path, value)
    }
}

fn load_one(path: &Path) -> AppResult<Validator> {
    let raw = std::fs::read_to_string(path).map_err(|e| AppError::SchemaValidation {
        path: path.display().to_string(),
        message: format!("cannot read schema: {e}"),
    })?;
    let json: Value = serde_json::from_str(&raw).map_err(|e| AppError::SchemaValidation {
        path: path.display().to_string(),
        message: format!("schema is not valid JSON: {e}"),
    })?;
    jsonschema::draft7::new(&json).map_err(|e| AppError::SchemaValidation {
        path: path.display().to_string(),
        message: format!("schema compile failed: {e}"),
    })
}

fn run(v: &Validator, path: &Path, value: &Value) -> AppResult<()> {
    let errors: Vec<String> = v.iter_errors(value).map(|e| format!("{e}")).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::SchemaValidation {
            path: path.display().to_string(),
            message: errors.join("; "),
        })
    }
}
