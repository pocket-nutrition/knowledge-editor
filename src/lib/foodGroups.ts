import type { FoodGroup } from "./types";

// Single source of truth for the food-group dropdown across the form and the
// list's bulk-update action. Keep this list aligned with the `FoodGroup`
// union in types.ts (and the matching enum in src-tauri/src/model.rs).
export const FOOD_GROUPS: readonly FoodGroup[] = [
  "meat",
  "fish",
  "shellfish",
  "vegetable",
  "cereal",
  "dairy",
  "beverage",
  "egg",
  "fat",
  "fruit",
  "legume",
  "nut_seed",
  "spice_herb",
  "sweetener",
  "baked_good",
  "sauce",
  "processed_meat",
  "prepared_dish",
  "seaweed",
  "alcohol",
  "other",
] as const;
