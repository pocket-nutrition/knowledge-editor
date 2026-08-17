import { invoke } from "@tauri-apps/api/core";
import type {
  DashboardStats,
  FoodGroup,
  Ingredient,
  IngredientDetailResponse,
  IngredientsListFilter,
  IngredientsListResponse,
  Source,
  SourceMapping,
  SourceMappingCookingMethod,
  StateKey,
  TypicalState,
} from "./types";

export const api = {
  dashboardStats: () => invoke<DashboardStats>("dashboard_stats"),

  listIngredients: (filter: IngredientsListFilter) =>
    invoke<IngredientsListResponse>("list_ingredients", { filter }),

  getIngredient: (id: string) =>
    invoke<IngredientDetailResponse>("get_ingredient", { id }),

  saveIngredient: (ingredient: Ingredient) =>
    invoke<Ingredient>("save_ingredient", { ingredient }),

  deleteIngredient: (id: string) => invoke<void>("delete_ingredient", { id }),

  // Copies every field of sourceId except id + name_en into a new ingredient.
  // No source mappings are carried over. Rejects a duplicate newId.
  cloneIngredient: (sourceId: string, newId: string, newNameEn: string) =>
    invoke<Ingredient>("clone_ingredient", { sourceId, newId, newNameEn }),

  setIngredientFoodGroup: (id: string, foodGroup: FoodGroup) =>
    invoke<Ingredient>("set_ingredient_food_group", {
      id,
      foodGroup,
    }),

  setIngredientDisplayUnit: (id: string, displayUnit: "g" | "ml" | null) =>
    invoke<Ingredient>("set_ingredient_display_unit", {
      id,
      displayUnit,
    }),

  // Bulk-edit helpers. `null` clears the field; an absent call leaves it
  // untouched. The backend rejects values outside the schema enum.
  setIngredientTypicalState: (id: string, typicalState: TypicalState | null) =>
    invoke<Ingredient>("set_ingredient_typical_state", {
      id,
      typicalState,
    }),

  setIngredientCanEatRaw: (id: string, value: boolean | null) =>
    invoke<Ingredient>("set_ingredient_can_eat_raw", { id, value }),

  setIngredientCanEatCooked: (id: string, value: boolean | null) =>
    invoke<Ingredient>("set_ingredient_can_eat_cooked", { id, value }),

  setIngredientStatePortion: (
    id: string,
    stateKey: StateKey,
    portionG: number | null,
  ) =>
    invoke<Ingredient>("set_ingredient_state_portion", {
      id,
      stateKey,
      portionG,
    }),

  setIngredientStateDensity: (
    id: string,
    stateKey: StateKey,
    densityGMl: number | null,
  ) =>
    invoke<Ingredient>("set_ingredient_state_density", {
      id,
      stateKey,
      densityGMl,
    }),

  unlinkMapping: (source: Source, sourceId: string) =>
    invoke<SourceMapping>("unlink_mapping", { source, sourceId }),

  markMappingComposite: (source: Source, sourceId: string) =>
    invoke<SourceMapping>("mark_mapping_composite", { source, sourceId }),

  validateMapping: (source: Source, sourceId: string) =>
    invoke<SourceMapping>("validate_mapping", { source, sourceId }),

  setMappingCookingMethod: (
    source: Source,
    sourceId: string,
    cookingMethod: SourceMappingCookingMethod,
  ) =>
    invoke<SourceMapping>("set_mapping_cooking_method", {
      source,
      sourceId,
      cookingMethod,
    }),

  linkMapping: (source: Source, sourceId: string, ingredientId: string) =>
    invoke<SourceMapping>("link_mapping", { source, sourceId, ingredientId }),

  searchUnlinkedMappings: (
    source: Source | null,
    query: string,
    limit?: number,
  ) =>
    invoke<SourceMapping[]>("search_unlinked_mappings", {
      source,
      query,
      limit,
    }),

  gitStatus: () =>
    invoke<{ modified: string[]; untracked: string[] }>("git_status"),

  triggerCommunityReload: () =>
    invoke<{ ok: boolean; detail?: string }>("trigger_community_reload"),
};
