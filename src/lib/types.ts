// Mirrors src-tauri/src/model.rs serde structs.
// Field shapes follow pocket-nutrition-knowledge/schemas/*.schema.json.

export type FoodGroup =
  | "meat"
  | "fish"
  | "shellfish"
  | "vegetable"
  | "cereal"
  | "dairy"
  | "beverage"
  | "egg"
  | "fat"
  | "fruit"
  | "legume"
  | "nut_seed"
  | "spice_herb"
  | "sweetener"
  | "baked_good"
  | "sauce"
  | "processed_meat"
  | "prepared_dish"
  | "seaweed"
  | "alcohol"
  | "other";

export type StateKey = "raw" | "cooked" | "processed";

export interface StateMetadata {
  portion_g?: number | null;
  density_g_ml?: number | null;
}

export type TypicalState =
  | StateKey
  | "boiled"
  | "steamed"
  | "grilled"
  | "roasted"
  | "fried";

export type CookingMethod =
  | "boiled"
  | "steamed"
  | "grilled"
  | "roasted"
  | "fried"
  | "cooked";

export type SourceMappingCookingMethod = CookingMethod | "raw" | "dry";

export type Source =
  | "ciqual"
  | "mccance"
  | "usda"
  | "usda_foundation"
  | "usda_sr"
  | "reference";

export type ServingMode = "item" | "weight" | "volume" | "measure";

export interface Ingredient {
  id: string;
  name_en: string;
  name_fr?: string;
  name_en_plural?: string;
  name_fr_plural?: string;
  // serving_mode = "item" → eaten by countable unit (egg, yogurt); the
  // unit_label_* give the per-unit name. Absent defaults to weight downstream.
  serving_mode?: ServingMode;
  unit_label_en?: string;
  unit_label_fr?: string;
  food_group: FoodGroup;
  states: Record<StateKey, StateMetadata>;
  notes?: string;
  can_eat_raw?: boolean;
  can_eat_cooked?: boolean;
  typical_state?: TypicalState;
  aliases_en?: string[];
  aliases_fr?: string[];
  serving_unit_types?: string[];
  display_unit?: "g" | "ml";
  absorption_factors?: Record<string, number>;
}

export interface SourceMapping {
  source: Source;
  source_id: string;
  canonical_ingredient_id?: string | null;
  label?: string;
  cooking_method?: SourceMappingCookingMethod;
  match_method?: string;
  confidence?: number;
  meta?: Record<string, string>;
  notes?: string;
}

export interface CookingRelationship {
  raw_id: string;
  cooking_method: CookingMethod;
  yield_factor: number;
  confidence?: number;
  source?: string;
  notes?: string;
  nutrient_retention?: Record<string, number>;
}

export interface CompletenessScore {
  overall: number;
  identity: number;
  sources: number;
  cooking: number;
  mapping_count: number;
  linked_mapping_count: number;
  validated_mapping_count: number;
  relationship_count: number;
  needs_cooking: boolean;
}

export interface IngredientSummary {
  id: string;
  name_en: string;
  name_fr?: string;
  food_group: FoodGroup;
  states: string[];
  display_unit?: "g" | "ml";
  completeness: CompletenessScore;
}

export interface IngredientsListResponse {
  items: IngredientSummary[];
  total: number;
  facets: { food_group: Record<string, number> };
}

export type IngredientSortKey =
  | "name"
  | "score_asc"
  | "score_desc"
  | "unvalidated_desc"
  | "unvalidated_asc";

export interface IngredientsListFilter {
  search?: string;
  food_group?: FoodGroup;
  offset?: number;
  limit?: number;
  orphans_filter?: "hide" | "only" | "all";
  sort?: IngredientSortKey;
}

export interface SourceStats {
  total: number;
  linked: number;
  composite: number;
  unlinked: number;
}

export interface DashboardStats {
  total_ingredients: number;
  by_food_group: Record<string, number>;
  total_mappings: number;
  mappings_by_source: Record<string, SourceStats>;
  total_relationships: number;
}

export interface IngredientDetailResponse {
  ingredient: Ingredient;
  mappings: SourceMapping[];
  relationships: CookingRelationship[];
}

export interface UnlinkedMapping extends SourceMapping {
  // Convenience flat fields surfaced by backend
}
