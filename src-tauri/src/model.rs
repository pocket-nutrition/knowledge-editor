//! Domain structs mirroring pocket-nutrition/knowledge YAML schemas.
//!
//! Schema authority lives in pocket-nutrition/knowledge/schemas/*.schema.json. These
//! Rust structs are the on-the-wire shape used by Tauri commands and by the
//! YAML round-trip; jsonschema is the validation layer enforced at write time.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FoodGroup {
    Meat,
    Fish,
    Shellfish,
    Vegetable,
    Cereal,
    Dairy,
    Beverage,
    Egg,
    Fat,
    Fruit,
    Legume,
    NutSeed,
    SpiceHerb,
    Sweetener,
    BakedGood,
    Sauce,
    ProcessedMeat,
    PreparedDish,
    Seaweed,
    Alcohol,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TypicalState {
    Raw,
    Cooked,
    Processed,
    Boiled,
    Steamed,
    Grilled,
    Roasted,
    Fried,
}

/// Natural measurement mode for an ingredient. `Item` = countable whole units
/// (egg, yogurt), which carry a `unit_label_en` / `unit_label_fr`. Absent
/// defaults to weight downstream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ServingMode {
    Item,
    Weight,
    Volume,
    Measure,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CookingMethod {
    Boiled,
    Steamed,
    Grilled,
    Roasted,
    Fried,
    Cooked,
}

impl CookingMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            CookingMethod::Boiled => "boiled",
            CookingMethod::Steamed => "steamed",
            CookingMethod::Grilled => "grilled",
            CookingMethod::Roasted => "roasted",
            CookingMethod::Fried => "fried",
            CookingMethod::Cooked => "cooked",
        }
    }
}

/// Cooking method on a SourceMapping is wider: it may also be "raw".
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SourceMappingCookingMethod {
    Raw,
    Boiled,
    Steamed,
    Grilled,
    Roasted,
    Fried,
    Cooked,
    // Serializes to "dry" — must match the source_mapping schema enum and the
    // frontend SourceMappingCookingMethod type (both use "dry", not "dried").
    Dry,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Ciqual,
    Mccance,
    Usda,
    UsdaFoundation,
    UsdaSr,
    Reference,
}

impl Source {
    pub fn as_dir(self) -> &'static str {
        match self {
            Source::Ciqual => "ciqual",
            Source::Mccance => "mccance",
            Source::Usda => "usda",
            Source::UsdaFoundation => "usda_foundation",
            Source::UsdaSr => "usda_sr",
            Source::Reference => "reference",
        }
    }

    pub fn all() -> &'static [Source] {
        &[
            Source::Ciqual,
            Source::Mccance,
            Source::Usda,
            Source::UsdaFoundation,
            Source::UsdaSr,
            Source::Reference,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StateMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portion_g: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density_g_ml: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ingredient {
    pub id: String,
    pub name_en: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_fr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_en_plural: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_fr_plural: Option<String>,
    /// Natural measurement mode (item / weight / volume / measure). When `Item`,
    /// `unit_label_en` / `unit_label_fr` give the per-unit label (e.g. egg).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serving_mode: Option<ServingMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_label_en: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_label_fr: Option<String>,
    pub food_group: FoodGroup,
    pub states: BTreeMap<String, StateMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can_eat_raw: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can_eat_cooked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typical_state: Option<TypicalState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aliases_en: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aliases_fr: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serving_unit_types: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub absorption_factors: Option<BTreeMap<String, f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMapping {
    pub source: Source,
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_ingredient_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooking_method: Option<SourceMappingCookingMethod>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl SourceMapping {
    /// "_composite" is the sentinel canonical_ingredient_id used to mark a
    /// source row as intentionally un-mappable to a single canonical
    /// ingredient (typically a multi-ingredient prepared dish).
    pub const COMPOSITE_SENTINEL: &'static str = "_composite";

    pub fn is_unlinked(&self) -> bool {
        self.canonical_ingredient_id.is_none()
            || self.canonical_ingredient_id.as_deref() == Some("")
    }

    pub fn is_composite(&self) -> bool {
        self.canonical_ingredient_id.as_deref() == Some(Self::COMPOSITE_SENTINEL)
    }

    pub fn is_linked(&self) -> bool {
        !self.is_unlinked() && !self.is_composite()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookingRelationship {
    pub raw_id: String,
    pub cooking_method: CookingMethod,
    pub yield_factor: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nutrient_retention: Option<BTreeMap<String, f64>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_mapping_unlinked_detection() {
        let m = SourceMapping {
            source: Source::Ciqual,
            source_id: "1".into(),
            canonical_ingredient_id: None,
            label: None,
            cooking_method: None,
            match_method: None,
            confidence: None,
            meta: None,
            notes: None,
        };
        assert!(m.is_unlinked());
        assert!(!m.is_composite());

        let m2 = SourceMapping {
            canonical_ingredient_id: Some("_composite".into()),
            ..m.clone()
        };
        assert!(!m2.is_unlinked());
        assert!(m2.is_composite());

        let m3 = SourceMapping {
            canonical_ingredient_id: Some("sweet_potato".into()),
            ..m
        };
        assert!(m3.is_linked());
    }
}
