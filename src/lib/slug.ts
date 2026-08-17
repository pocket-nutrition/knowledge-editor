// Derive a canonical ingredient id from a free-text name.
// Ingredient ids must match the schema pattern ^[a-z0-9_]{1,128}$.
export function slugify(name: string): string {
  return name
    .normalize("NFD")
    // strip combining diacritical marks (é → e, ç → c, …)
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 128);
}
