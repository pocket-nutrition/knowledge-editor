import { useEffect, useState } from "react";
import { api } from "../lib/tauri";
import type {
  IngredientSummary,
  StateKey,
  TypicalState,
} from "../lib/types";

type Mode = "direct" | "template";

type BoolValue = "true" | "false" | "clear";
type TypicalStateValue = TypicalState | "clear";

const TYPICAL_STATES: TypicalState[] = [
  "raw",
  "cooked",
  "processed",
  "boiled",
  "steamed",
  "grilled",
  "roasted",
  "fried",
];

const STATE_KEYS: StateKey[] = ["raw", "cooked", "processed"];

// Per-field include flags. Only flags set to `true` participate in the
// bulk write; everything else is left untouched on the target ingredients.
interface IncludeFlags {
  typical_state: boolean;
  can_eat_raw: boolean;
  can_eat_cooked: boolean;
  portion: Record<StateKey, boolean>;
  density: Record<StateKey, boolean>;
}

// Direct-mode form values. `"clear"` sentinels mean "drop the field on disk".
interface DirectFormState {
  typical_state: TypicalStateValue;
  can_eat_raw: BoolValue;
  can_eat_cooked: BoolValue;
  portion: Record<StateKey, { value: string; clear: boolean }>;
  density: Record<StateKey, { value: string; clear: boolean }>;
}

const initialIncludes = (): IncludeFlags => ({
  typical_state: false,
  can_eat_raw: false,
  can_eat_cooked: false,
  portion: { raw: false, cooked: false, processed: false },
  density: { raw: false, cooked: false, processed: false },
});

const initialDirectForm = (): DirectFormState => ({
  typical_state: "raw",
  can_eat_raw: "true",
  can_eat_cooked: "true",
  portion: {
    raw: { value: "", clear: false },
    cooked: { value: "", clear: false },
    processed: { value: "", clear: false },
  },
  density: {
    raw: { value: "", clear: false },
    cooked: { value: "", clear: false },
    processed: { value: "", clear: false },
  },
});

interface BulkEditPanelProps {
  selectedIds: string[];
  selectedItems: IngredientSummary[];
  onClose: () => void;
  onApplied: () => void;
}

export function BulkEditPanel({
  selectedIds,
  selectedItems,
  onClose,
  onApplied,
}: BulkEditPanelProps) {
  const [mode, setMode] = useState<Mode>("direct");
  const [includes, setIncludes] = useState<IncludeFlags>(initialIncludes());
  const [form, setForm] = useState<DirectFormState>(initialDirectForm());
  const [templateSourceId, setTemplateSourceId] = useState<string>("");
  const [templateIngredient, setTemplateIngredient] = useState<
    Awaited<ReturnType<typeof api.getIngredient>> | null
  >(null);
  const [templateError, setTemplateError] = useState<string | null>(null);

  const [busy, setBusy] = useState(false);
  const [failures, setFailures] = useState<string[]>([]);
  const [armed, setArmed] = useState(false);

  // Fetch the template source on selection so we can populate the include
  // checklist with its actual values.
  useEffect(() => {
    if (mode !== "template" || !templateSourceId) {
      setTemplateIngredient(null);
      setTemplateError(null);
      return;
    }
    let cancelled = false;
    api
      .getIngredient(templateSourceId)
      .then((detail) => {
        if (!cancelled) {
          setTemplateIngredient(detail);
          setTemplateError(null);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setTemplateError(String(e));
          setTemplateIngredient(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [mode, templateSourceId]);

  const targetIds =
    mode === "template"
      ? selectedIds.filter((id) => id !== templateSourceId)
      : selectedIds;

  const apply = async () => {
    if (targetIds.length === 0) return;
    setBusy(true);
    setFailures([]);
    setArmed(false);

    const tasks: Array<() => Promise<unknown>> = [];

    const pushTypicalState = (value: TypicalState | null) => {
      if (!includes.typical_state) return;
      for (const id of targetIds) {
        tasks.push(() => api.setIngredientTypicalState(id, value));
      }
    };
    const pushCanEatRaw = (value: boolean | null) => {
      if (!includes.can_eat_raw) return;
      for (const id of targetIds) {
        tasks.push(() => api.setIngredientCanEatRaw(id, value));
      }
    };
    const pushCanEatCooked = (value: boolean | null) => {
      if (!includes.can_eat_cooked) return;
      for (const id of targetIds) {
        tasks.push(() => api.setIngredientCanEatCooked(id, value));
      }
    };
    const pushPortion = (sk: StateKey, value: number | null) => {
      if (!includes.portion[sk]) return;
      for (const id of targetIds) {
        tasks.push(() => api.setIngredientStatePortion(id, sk, value));
      }
    };
    const pushDensity = (sk: StateKey, value: number | null) => {
      if (!includes.density[sk]) return;
      for (const id of targetIds) {
        tasks.push(() => api.setIngredientStateDensity(id, sk, value));
      }
    };

    if (mode === "direct") {
      if (includes.typical_state) {
        const v: TypicalState | null =
          form.typical_state === "clear" ? null : form.typical_state;
        pushTypicalState(v);
      }
      if (includes.can_eat_raw) {
        pushCanEatRaw(boolFromValue(form.can_eat_raw));
      }
      if (includes.can_eat_cooked) {
        pushCanEatCooked(boolFromValue(form.can_eat_cooked));
      }
      for (const sk of STATE_KEYS) {
        if (includes.portion[sk]) {
          const f = form.portion[sk];
          if (f.clear) {
            pushPortion(sk, null);
          } else {
            const parsed = parsePositiveNumber(f.value);
            if (parsed === null) {
              setFailures([`portion_g[${sk}]: invalid number "${f.value}"`]);
              setBusy(false);
              return;
            }
            pushPortion(sk, parsed);
          }
        }
        if (includes.density[sk]) {
          const f = form.density[sk];
          if (f.clear) {
            pushDensity(sk, null);
          } else {
            const parsed = parsePositiveNumber(f.value);
            if (parsed === null) {
              setFailures([`density_g_ml[${sk}]: invalid number "${f.value}"`]);
              setBusy(false);
              return;
            }
            pushDensity(sk, parsed);
          }
        }
      }
    } else if (mode === "template" && templateIngredient) {
      const src = templateIngredient.ingredient;
      if (includes.typical_state) {
        pushTypicalState(src.typical_state ?? null);
      }
      if (includes.can_eat_raw) {
        pushCanEatRaw(src.can_eat_raw ?? null);
      }
      if (includes.can_eat_cooked) {
        pushCanEatCooked(src.can_eat_cooked ?? null);
      }
      for (const sk of STATE_KEYS) {
        const meta = src.states?.[sk];
        if (includes.portion[sk]) {
          pushPortion(sk, meta?.portion_g ?? null);
        }
        if (includes.density[sk]) {
          pushDensity(sk, meta?.density_g_ml ?? null);
        }
      }
    }

    if (tasks.length === 0) {
      setBusy(false);
      return;
    }

    const results = await Promise.allSettled(tasks.map((fn) => fn()));
    const fails: string[] = [];
    results.forEach((r) => {
      if (r.status === "rejected") {
        fails.push(String(r.reason));
      }
    });

    setBusy(false);
    setFailures(fails);
    onApplied();
  };

  const totalTasks = countIncludedFields(includes) * targetIds.length;

  return (
    <div className="mt-2 p-3 rounded-md border border-[var(--color-border)] bg-white">
      <div className="flex items-center gap-3 mb-3">
        <div className="text-sm font-semibold">Bulk edit</div>
        <div className="text-xs text-[var(--color-text-muted)]">
          {targetIds.length} target{targetIds.length === 1 ? "" : "s"}
          {mode === "template" && templateSourceId
            ? ` (excluding template ${templateSourceId})`
            : ""}
        </div>
        <span className="flex-1" />
        <div className="flex items-center gap-1 text-xs">
          <button
            type="button"
            onClick={() => setMode("direct")}
            className={`px-2 py-1 rounded border ${
              mode === "direct"
                ? "border-[var(--color-accent)] text-[var(--color-accent)] bg-[var(--color-surface-2)]"
                : "border-[var(--color-border)] text-[var(--color-text-muted)]"
            }`}
            title="Enter values manually and apply to the selection"
          >
            Set values
          </button>
          <button
            type="button"
            onClick={() => setMode("template")}
            className={`px-2 py-1 rounded border ${
              mode === "template"
                ? "border-[var(--color-accent)] text-[var(--color-accent)] bg-[var(--color-surface-2)]"
                : "border-[var(--color-border)] text-[var(--color-text-muted)]"
            }`}
            title="Pick one selected ingredient as the source and copy its fields to the rest"
          >
            Copy from one
          </button>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="text-xs px-2 py-1 rounded border border-[var(--color-border)] text-[var(--color-text-muted)] hover:bg-[var(--color-surface-2)]"
        >
          Close
        </button>
      </div>

      {mode === "template" && (
        <div className="mb-3">
          <label className="text-xs uppercase tracking-wide text-[var(--color-text-muted)] block mb-1">
            Template source
          </label>
          <select
            value={templateSourceId}
            onChange={(e) => setTemplateSourceId(e.currentTarget.value)}
            className="w-full px-2 py-1 text-sm rounded border border-[var(--color-border)] bg-white"
          >
            <option value="">— pick one of the selected ingredients —</option>
            {selectedItems.map((i) => (
              <option key={i.id} value={i.id}>
                {i.id} · {i.name_en}
              </option>
            ))}
          </select>
          {templateError && (
            <div className="text-xs text-[var(--color-danger)] mt-1">
              {templateError}
            </div>
          )}
        </div>
      )}

      <div className="grid grid-cols-1 md:grid-cols-2 gap-x-6 gap-y-3 text-sm">
        <FieldRow
          label="typical_state"
          included={includes.typical_state}
          onToggle={() =>
            setIncludes((p) => ({ ...p, typical_state: !p.typical_state }))
          }
          preview={
            mode === "template"
              ? (templateIngredient?.ingredient.typical_state ?? "—")
              : undefined
          }
        >
          {mode === "direct" && (
            <select
              value={form.typical_state}
              onChange={(e) =>
                setForm((p) => ({
                  ...p,
                  typical_state: e.currentTarget.value as TypicalStateValue,
                }))
              }
              disabled={!includes.typical_state}
              className="px-2 py-1 text-xs rounded border border-[var(--color-border)] bg-white disabled:opacity-50"
            >
              {TYPICAL_STATES.map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
              <option value="clear">— clear</option>
            </select>
          )}
        </FieldRow>

        <FieldRow
          label="can_eat_raw"
          included={includes.can_eat_raw}
          onToggle={() =>
            setIncludes((p) => ({ ...p, can_eat_raw: !p.can_eat_raw }))
          }
          preview={
            mode === "template"
              ? formatBool(templateIngredient?.ingredient.can_eat_raw)
              : undefined
          }
        >
          {mode === "direct" && (
            <BoolSelect
              value={form.can_eat_raw}
              disabled={!includes.can_eat_raw}
              onChange={(v) => setForm((p) => ({ ...p, can_eat_raw: v }))}
            />
          )}
        </FieldRow>

        <FieldRow
          label="can_eat_cooked"
          included={includes.can_eat_cooked}
          onToggle={() =>
            setIncludes((p) => ({
              ...p,
              can_eat_cooked: !p.can_eat_cooked,
            }))
          }
          preview={
            mode === "template"
              ? formatBool(templateIngredient?.ingredient.can_eat_cooked)
              : undefined
          }
        >
          {mode === "direct" && (
            <BoolSelect
              value={form.can_eat_cooked}
              disabled={!includes.can_eat_cooked}
              onChange={(v) => setForm((p) => ({ ...p, can_eat_cooked: v }))}
            />
          )}
        </FieldRow>
      </div>

      <div className="mt-4">
        <div className="text-xs uppercase tracking-wide text-[var(--color-text-muted)] mb-2">
          State metadata (per state)
        </div>
        <div className="border border-[var(--color-border)] rounded overflow-hidden">
          <table className="w-full text-xs">
            <thead className="bg-[var(--color-surface-2)] text-[var(--color-text-muted)]">
              <tr>
                <th className="px-2 py-1 text-left w-20">state</th>
                <th className="px-2 py-1 text-left">portion_g</th>
                <th className="px-2 py-1 text-left">density_g_ml</th>
              </tr>
            </thead>
            <tbody>
              {STATE_KEYS.map((sk) => {
                const tplMeta = templateIngredient?.ingredient.states?.[sk];
                return (
                  <tr key={sk} className="border-t border-[var(--color-border)]">
                    <td className="px-2 py-1 font-mono">{sk}</td>
                    <td className="px-2 py-1">
                      <StateMetaCell
                        included={includes.portion[sk]}
                        onToggle={() =>
                          setIncludes((p) => ({
                            ...p,
                            portion: { ...p.portion, [sk]: !p.portion[sk] },
                          }))
                        }
                        preview={
                          mode === "template"
                            ? formatNumber(tplMeta?.portion_g)
                            : undefined
                        }
                        directInput={
                          mode === "direct" ? (
                            <NumberWithClear
                              disabled={!includes.portion[sk]}
                              value={form.portion[sk].value}
                              clear={form.portion[sk].clear}
                              onChange={(value, clear) =>
                                setForm((p) => ({
                                  ...p,
                                  portion: {
                                    ...p.portion,
                                    [sk]: { value, clear },
                                  },
                                }))
                              }
                            />
                          ) : null
                        }
                      />
                    </td>
                    <td className="px-2 py-1">
                      <StateMetaCell
                        included={includes.density[sk]}
                        onToggle={() =>
                          setIncludes((p) => ({
                            ...p,
                            density: { ...p.density, [sk]: !p.density[sk] },
                          }))
                        }
                        preview={
                          mode === "template"
                            ? formatNumber(tplMeta?.density_g_ml)
                            : undefined
                        }
                        directInput={
                          mode === "direct" ? (
                            <NumberWithClear
                              disabled={!includes.density[sk]}
                              value={form.density[sk].value}
                              clear={form.density[sk].clear}
                              onChange={(value, clear) =>
                                setForm((p) => ({
                                  ...p,
                                  density: {
                                    ...p.density,
                                    [sk]: { value, clear },
                                  },
                                }))
                              }
                            />
                          ) : null
                        }
                      />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>

      <div className="mt-3 flex items-center gap-2">
        <span className="text-xs text-[var(--color-text-muted)] font-mono">
          {totalTasks} write{totalTasks === 1 ? "" : "s"} queued
        </span>
        <span className="flex-1" />
        <button
          type="button"
          onClick={() => {
            if (busy) return;
            // Arm twice for ≥5 targets so a stray click can't blast the
            // whole selection — same pattern the delete button uses.
            if (targetIds.length >= 5 && !armed) {
              setArmed(true);
              window.setTimeout(() => setArmed(false), 3000);
              return;
            }
            void apply();
          }}
          disabled={
            busy ||
            totalTasks === 0 ||
            targetIds.length === 0 ||
            (mode === "template" && !templateSourceId)
          }
          className={`px-3 py-1.5 text-xs rounded border disabled:opacity-50 ${
            armed
              ? "border-[var(--color-warning)] bg-orange-100 text-[var(--color-warning)] font-semibold"
              : "border-[var(--color-accent)] text-[var(--color-accent)] hover:bg-[var(--color-surface-2)]"
          }`}
          title={
            mode === "template"
              ? "Copy the included fields from the template to the rest of the selection"
              : "Apply the included fields to every selected ingredient"
          }
        >
          {busy
            ? "Applying…"
            : armed
              ? `Confirm apply to ${targetIds.length}?`
              : `Apply (${targetIds.length})`}
        </button>
      </div>

      {failures.length > 0 && (
        <div className="mt-2 text-xs text-[var(--color-danger)] break-words">
          {failures.length} failure{failures.length === 1 ? "" : "s"}:{" "}
          {failures.slice(0, 5).join(" · ")}
          {failures.length > 5 ? " · …" : ""}
        </div>
      )}
    </div>
  );
}

function countIncludedFields(inc: IncludeFlags): number {
  let n = 0;
  if (inc.typical_state) n += 1;
  if (inc.can_eat_raw) n += 1;
  if (inc.can_eat_cooked) n += 1;
  for (const sk of STATE_KEYS) {
    if (inc.portion[sk]) n += 1;
    if (inc.density[sk]) n += 1;
  }
  return n;
}

function boolFromValue(v: BoolValue): boolean | null {
  if (v === "clear") return null;
  return v === "true";
}

function parsePositiveNumber(s: string): number | null {
  const trimmed = s.trim();
  if (trimmed === "") return null;
  const n = Number(trimmed);
  if (!Number.isFinite(n) || n <= 0) return null;
  return n;
}

function formatBool(v: boolean | null | undefined): string {
  if (v === undefined || v === null) return "—";
  return v ? "yes" : "no";
}

function formatNumber(v: number | null | undefined): string {
  if (v === undefined || v === null) return "—";
  return String(v);
}

interface FieldRowProps {
  label: string;
  included: boolean;
  onToggle: () => void;
  preview?: string;
  children?: React.ReactNode;
}

function FieldRow({
  label,
  included,
  onToggle,
  preview,
  children,
}: FieldRowProps) {
  return (
    <label className="flex items-center gap-2">
      <input
        type="checkbox"
        checked={included}
        onChange={onToggle}
        aria-label={`Include ${label}`}
      />
      <span className="text-xs font-mono w-32 shrink-0">{label}</span>
      {children}
      {preview !== undefined && (
        <span className="text-xs text-[var(--color-text-muted)] font-mono">
          → {preview}
        </span>
      )}
    </label>
  );
}

interface BoolSelectProps {
  value: BoolValue;
  disabled: boolean;
  onChange: (v: BoolValue) => void;
}

function BoolSelect({ value, disabled, onChange }: BoolSelectProps) {
  return (
    <select
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(e.currentTarget.value as BoolValue)}
      className="px-2 py-1 text-xs rounded border border-[var(--color-border)] bg-white disabled:opacity-50"
    >
      <option value="true">true</option>
      <option value="false">false</option>
      <option value="clear">— clear</option>
    </select>
  );
}

interface StateMetaCellProps {
  included: boolean;
  onToggle: () => void;
  preview?: string;
  directInput: React.ReactNode | null;
}

function StateMetaCell({
  included,
  onToggle,
  preview,
  directInput,
}: StateMetaCellProps) {
  return (
    <div className="flex items-center gap-2">
      <input
        type="checkbox"
        checked={included}
        onChange={onToggle}
        aria-label="Include field"
      />
      {directInput}
      {preview !== undefined && (
        <span className="text-xs text-[var(--color-text-muted)] font-mono">
          → {preview}
        </span>
      )}
    </div>
  );
}

interface NumberWithClearProps {
  value: string;
  clear: boolean;
  disabled: boolean;
  onChange: (value: string, clear: boolean) => void;
}

function NumberWithClear({
  value,
  clear,
  disabled,
  onChange,
}: NumberWithClearProps) {
  return (
    <div className="flex items-center gap-1">
      <input
        type="text"
        inputMode="decimal"
        value={value}
        disabled={disabled || clear}
        onChange={(e) => onChange(e.currentTarget.value, false)}
        placeholder="value"
        className="w-20 px-1 py-0.5 text-xs rounded border border-[var(--color-border)] bg-white disabled:opacity-50"
      />
      <label
        className={`text-xs flex items-center gap-1 ${
          disabled ? "opacity-50" : ""
        }`}
        title="Drop the field from disk (skip_serializing_if)"
      >
        <input
          type="checkbox"
          checked={clear}
          disabled={disabled}
          onChange={(e) => onChange(value, e.currentTarget.checked)}
        />
        clear
      </label>
    </div>
  );
}
