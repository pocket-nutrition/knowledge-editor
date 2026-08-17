import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { api } from "../lib/tauri";
import type {
  IngredientDetailResponse,
  SourceMapping,
  SourceMappingCookingMethod,
} from "../lib/types";
import { IngredientForm } from "../components/ingredient-edit/IngredientForm";
import { LinkMappingPanel } from "../components/ingredient-edit/LinkMappingPanel";
import { IngredientPicker } from "../components/ingredient-edit/IngredientPicker";
import { CloneIngredientPanel } from "../components/ingredient-edit/CloneIngredientPanel";

type Mode = "view" | "edit";

export function IngredientDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [data, setData] = useState<IngredientDetailResponse | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [mode, setMode] = useState<Mode>("view");
  const [busyMapping, setBusyMapping] = useState<string | null>(null);
  const [mappingError, setMappingError] = useState<string | null>(null);
  const [linkingOpen, setLinkingOpen] = useState(false);
  const [cloneOpen, setCloneOpen] = useState(false);

  const fetchDetail = useCallback(async () => {
    if (!id) return;
    try {
      const next = await api.getIngredient(id);
      setData(next);
      setErr(null);
    } catch (e) {
      setErr(String(e));
    }
  }, [id]);

  useEffect(() => {
    setData(null);
    setErr(null);
    setMode("view");
    setMappingError(null);
    fetchDetail();
  }, [fetchDetail]);

  if (err) {
    return <div className="p-6 text-[var(--color-danger)]">Error: {err}</div>;
  }

  if (!data) {
    return (
      <div className="p-6 text-[var(--color-text-muted)]">Loading…</div>
    );
  }

  const { ingredient, mappings, relationships } = data;

  const COOKING_METHOD_ORDER = [
    "raw",
    "boiled",
    "steamed",
    "grilled",
    "roasted",
    "fried",
    "cooked",
    "dry",
  ] as const;

  const groupedMappings = Object.entries(
    mappings.reduce<Record<string, SourceMapping[]>>(
      (acc, m) => {
        const key = m.cooking_method ?? "raw";
        (acc[key] ||= []).push(m);
        return acc;
      },
      {},
    ),
  )
    .sort(([a], [b]) => {
      const ia = COOKING_METHOD_ORDER.indexOf(
        a as (typeof COOKING_METHOD_ORDER)[number],
      );
      const ib = COOKING_METHOD_ORDER.indexOf(
        b as (typeof COOKING_METHOD_ORDER)[number],
      );
      return (ia === -1 ? 999 : ia) - (ib === -1 ? 999 : ib);
    })
    .map(
      ([method, list]) =>
        [
          method,
          [...list].sort((a, b) => a.source_id.localeCompare(b.source_id)),
        ] as [string, SourceMapping[]],
    );

  const runMappingAction = async (
    key: string,
    action: () => Promise<unknown>,
  ) => {
    setBusyMapping(key);
    setMappingError(null);
    try {
      await action();
      await fetchDetail();
    } catch (e) {
      setMappingError(String(e));
    } finally {
      setBusyMapping(null);
    }
  };

  const mappingsSection = (
    <section>
      <h2 className="text-sm font-semibold uppercase tracking-wide text-[var(--color-text-muted)] mb-2">
        Source mappings ({mappings.length})
      </h2>
      {mappingError && (
        <div className="mb-2 p-2 rounded bg-red-50 text-[var(--color-danger)] text-sm">
          {mappingError}
        </div>
      )}
      {mappings.length === 0 ? (
        <div className="text-sm text-[var(--color-text-muted)]">
          No source mappings link to this ingredient.
        </div>
      ) : (
        groupedMappings.map(([method, list]) => (
          <div key={method} className="mb-3">
            <div className="flex items-center gap-2 mb-1">
              <span className="text-xs uppercase tracking-wide text-[var(--color-text-muted)]">
                {method}
              </span>
              <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-[var(--color-surface-2)] text-[var(--color-text-muted)]">
                {list.length}
              </span>
            </div>
            <ul className="border border-[var(--color-border)] rounded-lg divide-y divide-[var(--color-border)]">
              {list.map((m) => (
                <MappingRow
                  key={`${m.source}-${m.source_id}`}
                  mapping={m}
                  busy={busyMapping === `${m.source}-${m.source_id}`}
                  onValidate={() =>
                    runMappingAction(`${m.source}-${m.source_id}`, () =>
                      api.validateMapping(m.source, m.source_id),
                    )
                  }
                  onUnlink={() =>
                    runMappingAction(`${m.source}-${m.source_id}`, () =>
                      api.unlinkMapping(m.source, m.source_id),
                    )
                  }
                  onComposite={() =>
                    runMappingAction(`${m.source}-${m.source_id}`, () =>
                      api.markMappingComposite(m.source, m.source_id),
                    )
                  }
                  onSetCookingMethod={(method) =>
                    runMappingAction(`${m.source}-${m.source_id}`, () =>
                      api.setMappingCookingMethod(m.source, m.source_id, method),
                    )
                  }
                  currentIngredientId={ingredient.id}
                  onTransfer={(targetId) =>
                    runMappingAction(`${m.source}-${m.source_id}`, () =>
                      api.linkMapping(m.source, m.source_id, targetId),
                    )
                  }
                />
              ))}
            </ul>
          </div>
        ))
      )}
      <div className="mt-2">
        <button
          type="button"
          onClick={() => setLinkingOpen((v) => !v)}
          className="text-xs px-2 py-1 rounded border border-[var(--color-border)] text-[var(--color-text-muted)] hover:bg-[var(--color-surface-2)]"
        >
          {linkingOpen ? "— Close" : "+ Add mapping"}
        </button>
        {linkingOpen && id && (
          <LinkMappingPanel
            ingredientId={id}
            onLinked={() => {
              fetchDetail();
            }}
          />
        )}
      </div>
    </section>
  );

  const relationshipsSection = (
    <section>
      <h2 className="text-sm font-semibold uppercase tracking-wide text-[var(--color-text-muted)] mb-2">
        Cooking relationships ({relationships.length})
      </h2>
      {relationships.length === 0 ? (
        <div className="text-sm text-[var(--color-text-muted)]">
          No cooking data.
        </div>
      ) : (
        <ul className="border border-[var(--color-border)] rounded-lg divide-y divide-[var(--color-border)]">
          {relationships.map((r) => (
            <li
              key={r.cooking_method}
              className="px-3 py-1.5 text-sm flex justify-between"
            >
              <span className="font-mono text-xs">{r.cooking_method}</span>
              <span className="text-[var(--color-text-muted)]">
                yield {r.yield_factor.toFixed(3)} · {r.source ?? "—"}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );

  if (mode === "edit") {
    return (
      <div className="p-6 max-w-6xl mx-auto space-y-6">
        <div className="flex items-start justify-between gap-4">
          <div>
            <button
              type="button"
              onClick={() => navigate(-1)}
              className="text-xs text-[var(--color-text-muted)] hover:underline"
            >
              ← back to list
            </button>
            <h1 className="text-2xl font-semibold mt-1">
              Edit · {ingredient.name_en}
            </h1>
            <div className="text-sm text-[var(--color-text-muted)] font-mono">
              {ingredient.id}
            </div>
          </div>
          <Link
            to="/ingredients"
            aria-label="Close"
            className="text-lg leading-none text-[var(--color-text-muted)] hover:text-[var(--color-text)] px-2 py-1 rounded hover:bg-[var(--color-surface-2)]"
          >
            ✕
          </Link>
        </div>
        <div className="grid grid-cols-1 lg:grid-cols-[minmax(0,1fr)_22rem] gap-6 items-start">
          <IngredientForm
            initial={ingredient}
            onSave={async (next) => {
              await api.saveIngredient(next);
              await fetchDetail();
              setMode("view");
            }}
            onCancel={() => {
              fetchDetail();
              setMode("view");
            }}
          />
          <aside className="space-y-6 lg:sticky lg:top-4">
            <div className="text-xs text-[var(--color-text-muted)] italic">
              Linked sources update independently of the form — saving the
              ingredient does not modify these mappings.
            </div>
            {mappingsSection}
            {relationshipsSection}
          </aside>
        </div>
      </div>
    );
  }

  return (
    <div className="p-6 max-w-5xl mx-auto space-y-6">
      <div className="flex items-start justify-between gap-4">
        <div>
          <Link
            to="/ingredients"
            className="text-xs text-[var(--color-text-muted)] hover:underline"
          >
            ← back to list
          </Link>
          <h1 className="text-2xl font-semibold mt-1">{ingredient.name_en}</h1>
          <div className="text-sm text-[var(--color-text-muted)]">
            {ingredient.name_fr} ·{" "}
            <span className="font-mono">{ingredient.id}</span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => setCloneOpen((v) => !v)}
            className={`px-3 py-1.5 rounded border text-sm ${cloneOpen ? "border-[var(--color-accent)] text-[var(--color-accent)] bg-[var(--color-surface-2)]" : "border-[var(--color-border)] hover:bg-[var(--color-surface-2)]"}`}
            title="Create a copy of this ingredient with a new id/name and no mappings"
          >
            Clone
          </button>
          <button
            type="button"
            onClick={() => setMode("edit")}
            className="px-3 py-1.5 rounded border border-[var(--color-border)] text-sm hover:bg-[var(--color-surface-2)]"
          >
            Edit
          </button>
          <Link
            to="/ingredients"
            aria-label="Close"
            className="text-lg leading-none text-[var(--color-text-muted)] hover:text-[var(--color-text)] px-2 py-1 rounded hover:bg-[var(--color-surface-2)]"
          >
            ✕
          </Link>
        </div>
      </div>

      {cloneOpen && (
        <CloneIngredientPanel
          sourceId={ingredient.id}
          sourceNameEn={ingredient.name_en}
          onClose={() => setCloneOpen(false)}
        />
      )}

      <section className="grid grid-cols-2 gap-x-8 gap-y-2 text-sm">
        <Field label="Food group" value={ingredient.food_group} />
        <Field
          label="States"
          value={Object.keys(ingredient.states).join(", ")}
        />
        <Field label="Typical state" value={ingredient.typical_state} />
        <Field
          label="Can eat raw"
          value={
            ingredient.can_eat_raw === undefined
              ? undefined
              : ingredient.can_eat_raw
                ? "yes"
                : "no"
          }
        />
        <Field
          label="Can eat cooked"
          value={
            ingredient.can_eat_cooked === undefined
              ? undefined
              : ingredient.can_eat_cooked
                ? "yes"
                : "no"
          }
        />
        <Field label="Display unit" value={ingredient.display_unit} />
        <Field
          label="Serving mode"
          value={
            ingredient.serving_mode === "item"
              ? `item (${ingredient.unit_label_en ?? "?"} / ${
                  ingredient.unit_label_fr ?? "?"
                })`
              : ingredient.serving_mode
          }
        />
      </section>

      <section>
        <h2 className="text-sm font-semibold uppercase tracking-wide text-[var(--color-text-muted)] mb-2">
          State metadata
        </h2>
        <ul className="border border-[var(--color-border)] rounded-lg divide-y divide-[var(--color-border)]">
          {Object.entries(ingredient.states).map(([state, meta]) => (
            <li key={state} className="px-3 py-1.5 text-sm flex gap-6">
              <span className="font-mono text-xs w-20 shrink-0 text-[var(--color-text-muted)] self-center">
                {state}
              </span>
              <span>
                {meta.portion_g != null ? `${meta.portion_g} g portion` : ""}
                {meta.portion_g != null && meta.density_g_ml != null
                  ? " · "
                  : ""}
                {meta.density_g_ml != null
                  ? `${meta.density_g_ml} g/mL`
                  : ""}
                {meta.portion_g == null && meta.density_g_ml == null
                  ? "—"
                  : ""}
              </span>
            </li>
          ))}
        </ul>
      </section>

      {(ingredient.aliases_en?.length || ingredient.aliases_fr?.length) && (
        <section className="grid grid-cols-2 gap-6 text-sm">
          <AliasList label="Aliases (EN)" value={ingredient.aliases_en} />
          <AliasList label="Aliases (FR)" value={ingredient.aliases_fr} />
        </section>
      )}

      {ingredient.notes && (
        <section>
          <h2 className="text-sm font-semibold uppercase tracking-wide text-[var(--color-text-muted)] mb-2">
            Notes
          </h2>
          <div className="text-sm whitespace-pre-wrap">{ingredient.notes}</div>
        </section>
      )}

      {mappingsSection}

      {relationshipsSection}
    </div>
  );
}

interface MappingRowProps {
  mapping: SourceMapping;
  busy: boolean;
  currentIngredientId: string;
  onValidate: () => void;
  onUnlink: () => void;
  onComposite: () => void;
  onSetCookingMethod: (method: SourceMappingCookingMethod) => void;
  onTransfer: (targetIngredientId: string) => void;
}

type PendingAction = "unlink" | "composite" | null;

function MappingRow({
  mapping,
  busy,
  currentIngredientId,
  onValidate,
  onUnlink,
  onComposite,
  onSetCookingMethod,
  onTransfer,
}: MappingRowProps) {
  const isValidated =
    mapping.match_method === "manual" && mapping.confidence === 1.0;
  const [pending, setPending] = useState<PendingAction>(null);
  const [transferOpen, setTransferOpen] = useState(false);
  const timerRef = useRef<number | null>(null);

  const armOrFire = (action: "unlink" | "composite") => {
    if (pending === action) {
      // Second click → fire.
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      setPending(null);
      if (action === "unlink") onUnlink();
      else onComposite();
      return;
    }
    setPending(action);
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      setPending(null);
      timerRef.current = null;
    }, 3000);
  };

  return (
    <li className="px-3 py-1.5 text-sm">
      <div className="flex items-center gap-3">
      <div className="flex-1 min-w-0">
        <div className="truncate">
          <span className="font-mono text-xs text-[var(--color-text-muted)] mr-2">
            {mapping.source}#{mapping.source_id}
          </span>
          {mapping.label}
        </div>
        <div className="text-xs text-[var(--color-text-muted)]">
          {mapping.match_method ?? "—"} ·{" "}
          {mapping.confidence != null ? mapping.confidence.toFixed(2) : "—"}
        </div>
      </div>
      <div className="flex items-center gap-1 shrink-0">
        <select
          value={mapping.cooking_method ?? "raw"}
          onChange={(e) =>
            onSetCookingMethod(e.target.value as SourceMappingCookingMethod)
          }
          disabled={busy}
          className="text-xs px-1 py-0.5 rounded border border-[var(--color-border)] bg-[var(--color-surface)] disabled:opacity-50"
          title="Change cooking method"
        >
          {(
            [
              "raw",
              "boiled",
              "steamed",
              "grilled",
              "roasted",
              "fried",
              "cooked",
              "dry",
            ] as SourceMappingCookingMethod[]
          ).map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
        {isValidated ? (
          <span
            className="px-2 py-0.5 text-xs rounded bg-[var(--color-success)] text-white"
            title="match_method=manual, confidence=1.0"
          >
            ✓ validated
          </span>
        ) : (
          <button
            type="button"
            onClick={onValidate}
            disabled={busy}
            className="px-2 py-0.5 text-xs rounded border border-[var(--color-success)] text-[var(--color-success)] hover:bg-green-50 disabled:opacity-50"
            title="Set match_method=manual, confidence=1.0"
          >
            Validate
          </button>
        )}
        <button
          type="button"
          onClick={() => armOrFire("unlink")}
          disabled={busy}
          className={`px-2 py-0.5 text-xs rounded border disabled:opacity-50 ${pending === "unlink" ? "border-[var(--color-warning)] bg-orange-100 text-[var(--color-warning)] font-semibold" : "border-[var(--color-warning)] text-[var(--color-warning)] hover:bg-orange-50"}`}
          title="Wrong match — return to unlinked pool (click twice to confirm)"
        >
          {pending === "unlink" ? "Confirm?" : "Unlink"}
        </button>
        <button
          type="button"
          onClick={() => armOrFire("composite")}
          disabled={busy}
          className={`px-2 py-0.5 text-xs rounded border disabled:opacity-50 ${pending === "composite" ? "border-[var(--color-text)] bg-[var(--color-surface-2)] text-[var(--color-text)] font-semibold" : "border-[var(--color-border)] text-[var(--color-text-muted)] hover:bg-[var(--color-surface-2)]"}`}
          title="Mark as composite (multi-ingredient, un-mappable) — click twice to confirm"
        >
          {pending === "composite" ? "Confirm?" : "Composite"}
        </button>
        <button
          type="button"
          onClick={() => setTransferOpen((v) => !v)}
          disabled={busy}
          className={`px-2 py-0.5 text-xs rounded border disabled:opacity-50 ${transferOpen ? "border-[var(--color-accent)] bg-blue-50 text-[var(--color-accent)] font-semibold" : "border-[var(--color-accent)] text-[var(--color-accent)] hover:bg-blue-50"}`}
          title="Move this mapping to a different canonical ingredient"
        >
          {transferOpen ? "— Close" : "Transfer"}
        </button>
      </div>
      </div>
      {transferOpen && (
        <IngredientPicker
          excludeId={currentIngredientId}
          busy={busy}
          onSelect={(targetId) => {
            setTransferOpen(false);
            onTransfer(targetId);
          }}
        />
      )}
    </li>
  );
}

function Field({
  label,
  value,
}: {
  label: string;
  value: string | undefined;
}) {
  return (
    <div>
      <div className="text-xs uppercase tracking-wide text-[var(--color-text-muted)]">
        {label}
      </div>
      <div>{value ?? "—"}</div>
    </div>
  );
}

function AliasList({
  label,
  value,
}: {
  label: string;
  value: string[] | undefined;
}) {
  if (!value || value.length === 0) return null;
  return (
    <div>
      <div className="text-xs uppercase tracking-wide text-[var(--color-text-muted)] mb-1">
        {label}
      </div>
      <div className="flex flex-wrap gap-1">
        {value.map((a) => (
          <span
            key={a}
            className="px-2 py-0.5 rounded bg-[var(--color-surface-2)] text-xs font-mono"
          >
            {a}
          </span>
        ))}
      </div>
    </div>
  );
}
