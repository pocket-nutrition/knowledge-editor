import { useState } from "react";
import { FOOD_GROUPS } from "../../lib/foodGroups";
import type {
  Ingredient,
  StateKey,
  StateMetadata,
  TypicalState,
} from "../../lib/types";
import {
  BoolField,
  CheckboxSetField,
  EnumSelect,
  FieldShell,
  KeyValueNumberField,
  NumberField,
  StringListField,
  TextAreaField,
  TextField,
} from "./fields";

const TYPICAL_STATES: readonly TypicalState[] = [
  "raw",
  "cooked",
  "processed",
  "boiled",
  "steamed",
  "grilled",
  "roasted",
  "fried",
] as const;

const STATE_KEYS: readonly StateKey[] = ["raw", "cooked", "processed"] as const;

const SERVING_UNIT_TYPES = [
  "pinch",
  "teaspoon",
  "tablespoon",
  "cup",
  "handful",
  "small_handful",
  "glass",
  "shot",
  "bowl",
  "scoop",
] as const;

const SERVING_MODES = ["item", "weight", "volume", "measure"] as const;

interface IngredientFormProps {
  initial: Ingredient;
  onSave: (next: Ingredient) => Promise<void>;
  onCancel: () => void;
}

export function IngredientForm({
  initial,
  onSave,
  onCancel,
}: IngredientFormProps) {
  const [draft, setDraft] = useState<Ingredient>(initial);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const patch = (p: Partial<Ingredient>) => setDraft((d) => ({ ...d, ...p }));

  const presentStates = Object.keys(draft.states ?? {}) as StateKey[];
  const canSave = !submitting && presentStates.length > 0;

  const submit = async () => {
    setSubmitting(true);
    setError(null);
    try {
      await onSave(draft);
    } catch (e) {
      setError(String(e));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="space-y-6">
      {error && (
        <div className="p-3 rounded border border-[var(--color-danger)] bg-red-50 text-sm text-[var(--color-danger)]">
          {error}
        </div>
      )}

      <section className="grid grid-cols-2 gap-x-6 gap-y-4">
        <TextField
          label="ID"
          value={draft.id}
          onChange={() => {}}
          required
          readOnly
        />
        <EnumSelect
          label="Food group"
          value={draft.food_group}
          options={FOOD_GROUPS}
          onChange={(v) => v && patch({ food_group: v })}
          required
          allowEmpty={false}
        />
        <TextField
          label="Name (EN)"
          value={draft.name_en}
          onChange={(v) => patch({ name_en: v ?? "" })}
          required
        />
        <TextField
          label="Name (FR)"
          value={draft.name_fr}
          onChange={(v) => patch({ name_fr: v })}
        />
        <TextField
          label="Name plural (EN)"
          value={draft.name_en_plural}
          onChange={(v) => patch({ name_en_plural: v })}
        />
        <TextField
          label="Name plural (FR)"
          value={draft.name_fr_plural}
          onChange={(v) => patch({ name_fr_plural: v })}
        />
        <EnumSelect
          label="Typical state"
          value={draft.typical_state}
          options={TYPICAL_STATES}
          onChange={(v) => patch({ typical_state: v })}
        />
        <EnumSelect
          label="Display unit"
          value={draft.display_unit ?? "g"}
          options={["g", "ml"] as const}
          onChange={(v) => patch({ display_unit: v })}
        />
        <EnumSelect
          label="Serving mode"
          value={draft.serving_mode}
          options={SERVING_MODES}
          onChange={(v) =>
            // Leaving "item" clears the now-irrelevant unit labels so they
            // don't linger in the YAML.
            patch(
              v === "item"
                ? { serving_mode: v }
                : {
                    serving_mode: v,
                    unit_label_en: undefined,
                    unit_label_fr: undefined,
                  },
            )
          }
        />
        <BoolField
          label="Can eat raw"
          value={draft.can_eat_raw}
          onChange={(v) => patch({ can_eat_raw: v })}
        />
        <BoolField
          label="Can eat cooked"
          value={draft.can_eat_cooked}
          onChange={(v) => patch({ can_eat_cooked: v })}
        />
      </section>

      {draft.serving_mode === "item" && (
        <section className="space-y-2 p-3 rounded border border-[var(--color-border)] bg-[var(--color-surface-2)]">
          <p className="text-xs text-[var(--color-text-muted)]">
            Optional. If set, these labels name one unit (e.g. “egg” / “oeuf”)
            so UIs can show “2 eggs”. Leave empty to skip — the unit weight
            comes from the state <code>portion_g</code> below regardless.
          </p>
          <div className="grid grid-cols-2 gap-x-6 gap-y-4">
            <TextField
              label="Unit label (EN) — optional"
              value={draft.unit_label_en}
              onChange={(v) => patch({ unit_label_en: v })}
              placeholder="egg"
            />
            <TextField
              label="Unit label (FR) — optional"
              value={draft.unit_label_fr}
              onChange={(v) => patch({ unit_label_fr: v })}
              placeholder="oeuf"
            />
          </div>
        </section>
      )}

      <FieldShell
        label="States"
        required
        hint="At least one preparation state with portion / density metadata."
      >
        <div className="space-y-2">
          {STATE_KEYS.map((key) => (
            <StateRow
              key={key}
              stateKey={key}
              meta={draft.states?.[key]}
              onToggle={(present) => {
                const next = { ...(draft.states ?? {}) } as Partial<
                  Record<StateKey, StateMetadata>
                >;
                if (present) {
                  next[key] = next[key] ?? {};
                } else {
                  delete next[key];
                }
                patch({ states: next as Record<StateKey, StateMetadata> });
              }}
              onChange={(meta) => {
                const next = {
                  ...(draft.states ?? {}),
                  [key]: meta,
                } as Record<StateKey, StateMetadata>;
                patch({ states: next });
              }}
            />
          ))}
        </div>
      </FieldShell>

      <TextAreaField
        label="Notes"
        value={draft.notes}
        onChange={(v) => patch({ notes: v })}
      />

      <div className="grid grid-cols-2 gap-x-6 gap-y-4">
        <StringListField
          label="Aliases (EN)"
          value={draft.aliases_en}
          onChange={(v) => patch({ aliases_en: v })}
        />
        <StringListField
          label="Aliases (FR)"
          value={draft.aliases_fr}
          onChange={(v) => patch({ aliases_fr: v })}
        />
        <CheckboxSetField
          label="Serving unit types"
          value={draft.serving_unit_types}
          options={SERVING_UNIT_TYPES}
          onChange={(v) => patch({ serving_unit_types: v })}
          hint="culinary units resolved via serving_unit_types.yaml"
        />
        <div />
      </div>

      <KeyValueNumberField
        label="Absorption factors"
        value={draft.absorption_factors}
        onChange={(v) => patch({ absorption_factors: v })}
      />

      <div className="flex items-center justify-end gap-2 pt-2">
        <button
          type="button"
          onClick={onCancel}
          className="px-3 py-1.5 rounded border border-[var(--color-border)] text-sm"
        >
          Cancel
        </button>
        <button
          type="button"
          onClick={submit}
          disabled={!canSave}
          className="px-3 py-1.5 rounded bg-[var(--color-accent)] text-white text-sm disabled:opacity-50"
          title={
            presentStates.length === 0
              ? "At least one state is required"
              : undefined
          }
        >
          {submitting ? "Saving…" : "Save"}
        </button>
      </div>
    </div>
  );
}

interface StateRowProps {
  stateKey: StateKey;
  meta: StateMetadata | undefined;
  onToggle: (present: boolean) => void;
  onChange: (meta: StateMetadata) => void;
}

function StateRow({ stateKey, meta, onToggle, onChange }: StateRowProps) {
  const present = meta !== undefined;
  return (
    <div className="flex items-end gap-3 p-2 rounded border border-[var(--color-border)] bg-[var(--color-surface-2)]">
      <label className="flex items-center gap-2 self-center w-28 shrink-0">
        <input
          type="checkbox"
          checked={present}
          onChange={(e) => {
            const next = e.currentTarget.checked;
            if (!next && present) {
              if (!window.confirm(`Remove "${stateKey}" state? This will clear its metadata.`)) return;
            }
            onToggle(next);
          }}
        />
        <span className="font-mono text-sm">{stateKey}</span>
      </label>
      <div className="flex-1">
        <NumberField
          label="portion_g"
          value={meta?.portion_g}
          onChange={(v) => present && onChange({ ...meta, portion_g: v })}
          min={1}
          max={5000}
        />
      </div>
      <div className="flex-1">
        <NumberField
          label="density_g_ml"
          value={meta?.density_g_ml}
          onChange={(v) => present && onChange({ ...meta, density_g_ml: v })}
          min={0.1}
          max={20}
        />
      </div>
    </div>
  );
}
