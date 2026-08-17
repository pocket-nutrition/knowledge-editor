import { useEffect, useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { BulkEditPanel } from "../components/BulkEditPanel";
import { FOOD_GROUPS } from "../lib/foodGroups";
import { api } from "../lib/tauri";
import type {
  CompletenessScore,
  FoodGroup,
  IngredientSortKey,
  IngredientSummary,
  IngredientsListResponse,
} from "../lib/types";

const PAGE_SIZE = 50;

type SortKey = IngredientSortKey;

function scoreColor(value: number): string {
  if (value >= 90) return "var(--color-success, #16a34a)";
  if (value >= 60) return "#84cc16";
  if (value >= 30) return "var(--color-warning, #f59e0b)";
  return "var(--color-danger, #dc2626)";
}

function dimensionTitle(c: CompletenessScore): string {
  return [
    `Identity ${c.identity}/100 — name_fr / typical_state / can_eat_*`,
    `Sources ${c.sources}/100 — ${c.validated_mapping_count} validated, ${c.linked_mapping_count} linked, ${c.mapping_count} total`,
    `Cooking ${c.cooking}/100 — ${c.relationship_count} relationship(s)${
      c.needs_cooking ? "" : " (N/A: raw only)"
    }`,
  ].join("\n");
}

export function IngredientsList() {
  const [searchParams, setSearchParams] = useSearchParams();
  const search = searchParams.get("q") ?? "";
  const foodGroup = (searchParams.get("group") ?? "") as FoodGroup | "";
  // Tri-state. Default = "hide": the curation backlog of unmapped ingredients
  // is hidden unless the curator opts in.
  const orphansMode = (
    ["hide", "only", "all"].includes(searchParams.get("orphans") ?? "")
      ? (searchParams.get("orphans") as "hide" | "only" | "all")
      : "hide"
  );
  const offset = Number(searchParams.get("offset") ?? "0");
  const sort = (searchParams.get("sort") ?? "name") as SortKey;

  const [debounced, setDebounced] = useState(search);

  const updateParam = (
    updates: Record<string, string | null>,
    resetOffset = false,
  ) => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        for (const [key, value] of Object.entries(updates)) {
          if (value == null || value === "") next.delete(key);
          else next.set(key, value);
        }
        if (resetOffset) next.delete("offset");
        return next;
      },
      { replace: true },
    );
  };

  const [data, setData] = useState<IngredientsListResponse | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [armed, setArmed] = useState(false);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [bulkGroup, setBulkGroup] = useState<FoodGroup | "">("");
  const [bulkBusy, setBulkBusy] = useState(false);
  const [bulkError, setBulkError] = useState<string | null>(null);
  // display_unit: "clear" maps to a null write (drop the field) so the
  // curator can undo an explicit choice without re-opening the form.
  const [bulkUnit, setBulkUnit] = useState<"" | "g" | "ml" | "clear">("");
  const [bulkUnitBusy, setBulkUnitBusy] = useState(false);
  const [bulkUnitError, setBulkUnitError] = useState<string | null>(null);
  const [bulkPanelOpen, setBulkPanelOpen] = useState(false);
  const [reloadTick, setReloadTick] = useState(0);

  useEffect(() => {
    const id = setTimeout(() => setDebounced(search), 200);
    return () => clearTimeout(id);
  }, [search]);

  useEffect(() => {
    setSelected(new Set());
    setArmed(false);
    setDeleteError(null);
    setBulkGroup("");
    setBulkError(null);
    setBulkUnit("");
    setBulkUnitError(null);
    setBulkPanelOpen(false);
  }, [debounced, foodGroup, orphansMode, offset]);

  useEffect(() => {
    setLoading(true);
    api
      .listIngredients({
        search: debounced || undefined,
        food_group: foodGroup || undefined,
        orphans_filter: orphansMode,
        sort,
        offset,
        limit: PAGE_SIZE,
      })
      .then(setData)
      .catch((e) => setErr(String(e)))
      .finally(() => setLoading(false));
  }, [debounced, foodGroup, orphansMode, sort, offset, reloadTick]);

  // The backend applies the sort across the whole filtered set before
  // paginating, so the page is already in the requested order.
  const rawItems: IngredientSummary[] = data?.items ?? [];
  const items = rawItems;
  const total = data?.total ?? 0;
  const facets = data?.facets.food_group ?? {};
  const pageEnd = Math.min(offset + PAGE_SIZE, total);
  const groupKeys = useMemo(
    () => Object.keys(facets).sort((a, b) => facets[b] - facets[a]),
    [facets],
  );

  // Average score across the currently filtered page — quick "is the corpus
  // healthy?" read for the curator. Empty page → no value to show.
  const pageAvg = useMemo(() => {
    if (rawItems.length === 0) return null;
    const sum = rawItems.reduce((acc, i) => acc + i.completeness.overall, 0);
    return Math.round(sum / rawItems.length);
  }, [rawItems]);

  const toggleSelect = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const allVisibleSelected =
    items.length > 0 && items.every((i) => selected.has(i.id));

  const toggleSelectAll = () =>
    setSelected((prev) => {
      if (allVisibleSelected) {
        const next = new Set(prev);
        items.forEach((i) => next.delete(i.id));
        return next;
      }
      const next = new Set(prev);
      items.forEach((i) => next.add(i.id));
      return next;
    });

  const armDelete = () => {
    if (armed) {
      void bulkDelete();
      return;
    }
    setArmed(true);
    setDeleteError(null);
    window.setTimeout(() => setArmed(false), 3000);
  };

  const bulkSetFoodGroup = async () => {
    if (selected.size === 0 || !bulkGroup) return;
    setBulkBusy(true);
    setBulkError(null);
    const ids = Array.from(selected);
    const failures: string[] = [];
    await Promise.all(
      ids.map((id) =>
        api.setIngredientFoodGroup(id, bulkGroup).catch((e) => {
          failures.push(`${id}: ${e}`);
        }),
      ),
    );
    setBulkBusy(false);
    if (failures.length > 0) {
      setBulkError(failures.join(" · "));
    }
    setSelected(new Set());
    setBulkGroup("");
    setReloadTick((t) => t + 1);
  };

  const bulkSetDisplayUnit = async () => {
    if (selected.size === 0 || !bulkUnit) return;
    setBulkUnitBusy(true);
    setBulkUnitError(null);
    const target: "g" | "ml" | null = bulkUnit === "clear" ? null : bulkUnit;
    const ids = Array.from(selected);
    const failures: string[] = [];
    await Promise.all(
      ids.map((id) =>
        api.setIngredientDisplayUnit(id, target).catch((e) => {
          failures.push(`${id}: ${e}`);
        }),
      ),
    );
    setBulkUnitBusy(false);
    if (failures.length > 0) {
      setBulkUnitError(failures.join(" · "));
    }
    setSelected(new Set());
    setBulkUnit("");
    setReloadTick((t) => t + 1);
  };

  const bulkDelete = async () => {
    if (selected.size === 0) return;
    setDeleteBusy(true);
    setArmed(false);
    const ids = Array.from(selected);
    const failures: string[] = [];
    await Promise.all(
      ids.map((id) =>
        api.deleteIngredient(id).catch((e) => {
          failures.push(`${id}: ${e}`);
        }),
      ),
    );
    setDeleteBusy(false);
    if (failures.length > 0) {
      setDeleteError(failures.join(" · "));
    }
    setSelected(new Set());
    setReloadTick((t) => t + 1);
  };

  if (err) {
    return (
      <div className="p-6 text-[var(--color-danger)]">Error: {err}</div>
    );
  }

  return (
    <div className="p-6 max-w-6xl mx-auto">
      <div className="flex items-center gap-3 mb-4">
        <input
          value={search}
          onChange={(e) =>
            updateParam({ q: e.currentTarget.value || null }, true)
          }
          placeholder="Search by name / alias / id…"
          className="flex-1 px-3 py-1.5 rounded-md border border-[var(--color-border)] bg-white"
        />
        <select
          value={foodGroup}
          onChange={(e) =>
            updateParam({ group: e.currentTarget.value || null }, true)
          }
          className="px-3 py-1.5 rounded-md border border-[var(--color-border)] bg-white text-sm"
        >
          <option value="">All food groups</option>
          {groupKeys.map((g) => (
            <option key={g} value={g}>
              {g} ({facets[g]})
            </option>
          ))}
        </select>
        <select
          value={sort}
          onChange={(e) => updateParam({ sort: e.currentTarget.value })}
          className="px-3 py-1.5 rounded-md border border-[var(--color-border)] bg-white text-sm"
          title="Sort order"
        >
          <option value="name">Sort: Name</option>
          <option value="score_asc">Sort: Score ↑ (worst first)</option>
          <option value="score_desc">Sort: Score ↓ (best first)</option>
          <option value="unvalidated_desc">
            Sort: Unvalidated ↓ (most first)
          </option>
          <option value="unvalidated_asc">
            Sort: Unvalidated ↑ (fewest first)
          </option>
        </select>
        <select
          value={orphansMode}
          onChange={(e) =>
            updateParam(
              {
                orphans:
                  e.currentTarget.value === "hide"
                    ? null
                    : e.currentTarget.value,
              },
              true,
            )
          }
          className="px-3 py-1.5 rounded-md border border-[var(--color-border)] bg-white text-sm"
          title="How to treat ingredients with zero source mappings"
        >
          <option value="hide">Hide orphans</option>
          <option value="only">Orphans only</option>
          <option value="all">Show all</option>
        </select>
        {pageAvg !== null && (
          <div
            className="text-xs font-mono px-2 py-1 rounded border border-[var(--color-border)]"
            title="Average completeness across the current page"
          >
            avg{" "}
            <span style={{ color: scoreColor(pageAvg), fontWeight: 600 }}>
              {pageAvg}%
            </span>
          </div>
        )}
        <div className="text-xs text-[var(--color-text-muted)] font-mono">
          {total === 0 ? "0" : `${offset + 1}–${pageEnd} / ${total}`}
        </div>
      </div>

      {selected.size > 0 && (
        <div className="sticky top-0 z-10 mb-2 p-2 rounded-md border border-[var(--color-border)] bg-[var(--color-surface-2)] flex flex-wrap items-center gap-2 text-sm">
          <span className="font-mono">{selected.size} selected</span>
          <span className="flex-1" />

          <div className="flex items-center gap-1">
            <select
              value={bulkGroup}
              onChange={(e) =>
                setBulkGroup(e.currentTarget.value as FoodGroup | "")
              }
              disabled={bulkBusy || bulkUnitBusy || deleteBusy}
              className="px-2 py-1 text-xs rounded border border-[var(--color-border)] bg-white disabled:opacity-50"
              title="Set food group for the selected ingredients"
            >
              <option value="">Change group…</option>
              {FOOD_GROUPS.map((g) => (
                <option key={g} value={g}>
                  {g}
                </option>
              ))}
            </select>
            <button
              type="button"
              onClick={bulkSetFoodGroup}
              disabled={bulkBusy || bulkUnitBusy || deleteBusy || !bulkGroup}
              className="px-2 py-1 text-xs rounded border border-[var(--color-accent)] text-[var(--color-accent)] hover:bg-[var(--color-surface-2)] disabled:opacity-50"
              title="Reclassify the selected ingredients into the chosen food group"
            >
              {bulkBusy ? "Applying…" : "Apply"}
            </button>
          </div>

          <div className="flex items-center gap-1">
            <select
              value={bulkUnit}
              onChange={(e) =>
                setBulkUnit(e.currentTarget.value as typeof bulkUnit)
              }
              disabled={bulkBusy || bulkUnitBusy || deleteBusy}
              className="px-2 py-1 text-xs rounded border border-[var(--color-border)] bg-white disabled:opacity-50"
              title="Set display unit (g / ml) for the selected ingredients"
            >
              <option value="">Set unit…</option>
              <option value="g">g</option>
              <option value="ml">ml</option>
              <option value="clear">— clear</option>
            </select>
            <button
              type="button"
              onClick={bulkSetDisplayUnit}
              disabled={
                bulkBusy || bulkUnitBusy || deleteBusy || !bulkUnit
              }
              className="px-2 py-1 text-xs rounded border border-[var(--color-accent)] text-[var(--color-accent)] hover:bg-[var(--color-surface-2)] disabled:opacity-50"
              title="Apply the chosen display unit to the selection"
            >
              {bulkUnitBusy ? "Applying…" : "Apply"}
            </button>
          </div>

          <button
            type="button"
            onClick={() => setBulkPanelOpen((v) => !v)}
            disabled={deleteBusy || bulkBusy || bulkUnitBusy}
            className={`px-2 py-1 text-xs rounded border disabled:opacity-50 ${
              bulkPanelOpen
                ? "border-[var(--color-accent)] text-[var(--color-accent)] bg-[var(--color-surface-2)]"
                : "border-[var(--color-border)]"
            }`}
            title="Bulk edit density, portion, can_eat_*, typical_state — or copy from one selected ingredient"
          >
            {bulkPanelOpen ? "Hide bulk edit" : "More fields…"}
          </button>

          <button
            type="button"
            onClick={() => {
              setSelected(new Set());
              setArmed(false);
              setDeleteError(null);
              setBulkError(null);
              setBulkUnitError(null);
              setBulkPanelOpen(false);
            }}
            disabled={deleteBusy || bulkBusy || bulkUnitBusy}
            className="px-2 py-1 text-xs rounded border border-[var(--color-border)] disabled:opacity-50"
          >
            Clear
          </button>
          <button
            type="button"
            onClick={armDelete}
            disabled={deleteBusy || bulkBusy || bulkUnitBusy}
            className={`px-2 py-1 text-xs rounded border disabled:opacity-50 ${
              armed
                ? "border-[var(--color-danger)] bg-red-100 text-[var(--color-danger)] font-semibold"
                : "border-[var(--color-danger)] text-[var(--color-danger)] hover:bg-red-50"
            }`}
            title="Delete the selected ingredient YAMLs (irreversible). Click twice to confirm."
          >
            {deleteBusy
              ? "Deleting…"
              : armed
                ? `Confirm delete ${selected.size}?`
                : `Delete ${selected.size}`}
          </button>

          {(deleteError || bulkError || bulkUnitError) && (
            <div className="basis-full text-xs text-[var(--color-danger)] truncate">
              {bulkUnitError ?? bulkError ?? deleteError}
            </div>
          )}
        </div>
      )}

      {selected.size > 0 && bulkPanelOpen && (
        <BulkEditPanel
          selectedIds={Array.from(selected)}
          selectedItems={rawItems.filter((i) => selected.has(i.id))}
          onClose={() => setBulkPanelOpen(false)}
          onApplied={() => {
            setSelected(new Set());
            setBulkPanelOpen(false);
            setReloadTick((t) => t + 1);
          }}
        />
      )}

      <div className="border border-[var(--color-border)] rounded-lg overflow-hidden">
        <table className="w-full text-sm">
          <thead className="bg-[var(--color-surface-2)] text-left text-[var(--color-text-muted)]">
            <tr>
              <th className="px-3 py-2 w-8">
                <input
                  type="checkbox"
                  checked={allVisibleSelected}
                  onChange={toggleSelectAll}
                  disabled={items.length === 0}
                  aria-label="Select all visible"
                />
              </th>
              <th className="px-3 py-2 w-44">ID</th>
              <th className="px-3 py-2">Name (EN)</th>
              <th className="px-3 py-2">Name (FR)</th>
              <th className="px-3 py-2 w-24">Group</th>
              <th className="px-3 py-2 w-20">State</th>
              <th className="px-3 py-2 w-12">Unit</th>
              <th
                className="px-3 py-2 w-32"
                title="Total source mappings · how many are not validated at 100%"
              >
                Mappings
              </th>
              <th className="px-3 py-2 w-56">Completeness</th>
            </tr>
          </thead>
          <tbody>
            {loading && items.length === 0 ? (
              <tr>
                <td
                  colSpan={9}
                  className="px-3 py-6 text-center text-[var(--color-text-muted)]"
                >
                  Loading…
                </td>
              </tr>
            ) : items.length === 0 ? (
              <tr>
                <td
                  colSpan={9}
                  className="px-3 py-6 text-center text-[var(--color-text-muted)]"
                >
                  No matches.
                </td>
              </tr>
            ) : (
              items.map((i) => (
                <tr
                  key={i.id}
                  className="border-t border-[var(--color-border)] hover:bg-[var(--color-surface-2)]"
                >
                  <td className="px-3 py-1.5">
                    <input
                      type="checkbox"
                      checked={selected.has(i.id)}
                      onChange={() => toggleSelect(i.id)}
                      aria-label={`Select ${i.id}`}
                    />
                  </td>
                  <td className="px-3 py-1.5 font-mono">
                    <Link
                      to={`/ingredients/${i.id}`}
                      className="text-[var(--color-accent)] hover:underline"
                    >
                      {i.id}
                    </Link>
                  </td>
                  <td className="px-3 py-1.5">{i.name_en}</td>
                  <td className="px-3 py-1.5 text-[var(--color-text-muted)]">
                    {i.name_fr ?? ""}
                  </td>
                  <td className="px-3 py-1.5 text-xs">{i.food_group}</td>
                  <td className="px-3 py-1.5 text-xs">{i.states.join(", ")}</td>
                  <td
                    className="px-3 py-1.5 text-xs font-mono"
                    title={
                      i.display_unit
                        ? "Explicit display_unit in YAML"
                        : "Implicit default — no display_unit in YAML, downstream treats as grams"
                    }
                  >
                    {i.display_unit ?? (
                      <span className="italic text-[var(--color-text-muted)]">
                        g
                      </span>
                    )}
                  </td>
                  <td className="px-3 py-1.5">
                    <MappingsCell score={i.completeness} />
                  </td>
                  <td className="px-3 py-1.5">
                    <CompletenessCell score={i.completeness} />
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <div className="flex items-center justify-between mt-3 text-sm">
        <button
          onClick={() => {
            const prev = offset - PAGE_SIZE;
            updateParam({ offset: prev > 0 ? String(prev) : null });
          }}
          disabled={offset === 0}
          className="px-3 py-1.5 rounded border border-[var(--color-border)] disabled:opacity-40"
        >
          ← Previous
        </button>
        <button
          onClick={() => updateParam({ offset: String(offset + PAGE_SIZE) })}
          disabled={pageEnd >= total}
          className="px-3 py-1.5 rounded border border-[var(--color-border)] disabled:opacity-40"
        >
          Next →
        </button>
      </div>
    </div>
  );
}

interface MappingsCellProps {
  score: CompletenessScore;
}

// Total mappings + how many are not validated at 100%
// (mapping_count − validated_mapping_count). The unvalidated badge is the
// curation backlog signal the list can be sorted by.
function MappingsCell({ score }: MappingsCellProps) {
  const unvalidated = Math.max(
    0,
    score.mapping_count - score.validated_mapping_count,
  );
  return (
    <div className="flex items-center gap-2 text-xs font-mono">
      <span
        className="text-[var(--color-text-muted)]"
        title={`${score.mapping_count} total · ${score.validated_mapping_count} validated · ${score.linked_mapping_count} linked`}
      >
        {score.mapping_count}
      </span>
      {unvalidated > 0 ? (
        <span
          className="px-1.5 py-0.5 rounded"
          style={{
            color: "var(--color-warning, #f59e0b)",
            border: "1px solid var(--color-warning, #f59e0b)",
          }}
          title={`${unvalidated} mapping(s) not validated at 100%`}
        >
          {unvalidated} ⚠
        </span>
      ) : score.mapping_count > 0 ? (
        <span
          className="px-1.5 py-0.5 rounded"
          style={{
            color: "var(--color-success, #16a34a)",
            border: "1px solid var(--color-success, #16a34a)",
          }}
          title="All mappings validated at 100%"
        >
          ✓
        </span>
      ) : (
        <span className="text-[var(--color-text-muted)] italic">—</span>
      )}
    </div>
  );
}

interface CompletenessCellProps {
  score: CompletenessScore;
}

function CompletenessCell({ score }: CompletenessCellProps) {
  const color = scoreColor(score.overall);
  return (
    <div
      className="flex items-center gap-2"
      title={dimensionTitle(score)}
    >
      <div className="flex-1 min-w-0">
        <div className="flex items-center justify-between gap-2 mb-0.5">
          <span
            className="text-xs font-mono"
            style={{ color, fontWeight: 600 }}
          >
            {score.overall}%
          </span>
          <div className="flex items-center gap-1">
            <DimensionChip label="I" value={score.identity} />
            <DimensionChip label="S" value={score.sources} />
            <DimensionChip
              label="C"
              value={score.cooking}
              dim={!score.needs_cooking}
            />
          </div>
        </div>
        <div
          className="h-1.5 rounded-full overflow-hidden bg-[var(--color-surface-2)]"
          role="progressbar"
          aria-valuenow={score.overall}
          aria-valuemin={0}
          aria-valuemax={100}
        >
          <div
            className="h-full transition-[width] duration-200"
            style={{ width: `${score.overall}%`, backgroundColor: color }}
          />
        </div>
      </div>
    </div>
  );
}

interface DimensionChipProps {
  label: string;
  value: number;
  dim?: boolean;
}

function DimensionChip({ label, value, dim }: DimensionChipProps) {
  const color = dim ? "var(--color-text-muted)" : scoreColor(value);
  return (
    <span
      className="text-[10px] font-mono px-1 rounded"
      style={{
        color,
        border: `1px solid ${color}`,
        opacity: dim ? 0.5 : 1,
      }}
    >
      {label}
    </span>
  );
}
