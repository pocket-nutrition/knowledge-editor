//! Resolves and validates the pocket-nutrition/knowledge repo path.
//!
//! Priority order:
//! 1. `POCKET_NUTRITION_KNOWLEDGE_PATH` env var (handy for dev / CI)
//! 2. `~/.config/pocket-nutrition-knowledge-editor/config.toml` -> `repo_path = "..."`
//! 3. Hard default: `~/dev/pocketnutrition/knowledge`
//!
//! This is a fresh public fork with no existing installs to preserve compatibility with, so the
//! env var and config directory were renamed to match the public `pocket-nutrition` naming — unlike
//! the internal copy, which intentionally kept its original naming to avoid orphaning an existing
//! config file or shell profile for no functional gain.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserConfig {
    pub repo_path: Option<String>,
    pub community_url: Option<String>,
    pub community_reload_token: Option<String>,
}

impl UserConfig {
    fn config_path() -> Option<PathBuf> {
        let dir = dirs::config_dir()?.join("pocket-nutrition-knowledge-editor");
        Some(dir.join("config.toml"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Repo {
    pub root: PathBuf,
    pub config: UserConfig,
}

impl Repo {
    pub fn discover() -> AppResult<Self> {
        let config = UserConfig::load();
        let root = resolve_root(&config)?;
        ensure_layout(&root)?;
        Ok(Self { root, config })
    }

    pub fn ingredients_dir(&self) -> PathBuf {
        self.root.join("ingredients")
    }

    pub fn mappings_dir(&self) -> PathBuf {
        self.root.join("source_mappings")
    }

    pub fn relationships_dir(&self) -> PathBuf {
        self.root.join("cooking_relationships")
    }

    pub fn schemas_dir(&self) -> PathBuf {
        self.root.join("schemas")
    }

    pub fn ingredient_path(&self, id: &str) -> PathBuf {
        self.ingredients_dir().join(format!("{id}.yaml"))
    }

    pub fn mapping_path(&self, source_dir: &str, source_id: &str) -> PathBuf {
        self.mappings_dir()
            .join(source_dir)
            .join(format!("{source_id}.yaml"))
    }
}

fn resolve_root(config: &UserConfig) -> AppResult<PathBuf> {
    if let Ok(p) = std::env::var("POCKET_NUTRITION_KNOWLEDGE_PATH") {
        if !p.trim().is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    if let Some(p) = &config.repo_path {
        if !p.trim().is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let home = dirs::home_dir()
        .context("home dir not found")
        .map_err(AppError::from)?;
    Ok(home.join("dev/pocketnutrition/knowledge"))
}

fn ensure_layout(root: &Path) -> AppResult<()> {
    if !root.is_dir() {
        return Err(AppError::RepoMissing(format!(
            "{} is not a directory",
            root.display()
        )));
    }
    for sub in [
        "ingredients",
        "source_mappings",
        "cooking_relationships",
        "schemas",
    ] {
        let p = root.join(sub);
        if !p.is_dir() {
            return Err(AppError::RepoMissing(format!(
                "missing required subdirectory: {}",
                p.display()
            )));
        }
    }
    Ok(())
}
