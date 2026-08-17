import type { ReactNode } from "react";

interface FieldShellProps {
  label: string;
  required?: boolean;
  hint?: string;
  children: ReactNode;
}

export function FieldShell({ label, required, hint, children }: FieldShellProps) {
  return (
    <label className="block">
      <div className="text-xs uppercase tracking-wide text-[var(--color-text-muted)] mb-1">
        {label}
        {required && (
          <span className="text-[var(--color-danger)] ml-0.5">*</span>
        )}
      </div>
      {children}
      {hint && (
        <div className="text-xs text-[var(--color-text-muted)] mt-1">{hint}</div>
      )}
    </label>
  );
}

interface TextFieldProps {
  label: string;
  value: string | undefined;
  onChange: (v: string | undefined) => void;
  required?: boolean;
  readOnly?: boolean;
  placeholder?: string;
}

export function TextField({
  label,
  value,
  onChange,
  required,
  readOnly,
  placeholder,
}: TextFieldProps) {
  return (
    <FieldShell label={label} required={required}>
      <input
        type="text"
        value={value ?? ""}
        placeholder={placeholder}
        readOnly={readOnly}
        onChange={(e) => {
          const v = e.currentTarget.value;
          onChange(required ? v : v === "" ? undefined : v);
        }}
        className={`w-full px-2 py-1 rounded border border-[var(--color-border)] bg-white text-sm ${readOnly ? "bg-[var(--color-surface-2)] cursor-not-allowed" : ""}`}
      />
    </FieldShell>
  );
}

interface TextAreaFieldProps {
  label: string;
  value: string | undefined;
  onChange: (v: string | undefined) => void;
  rows?: number;
}

export function TextAreaField({
  label,
  value,
  onChange,
  rows = 3,
}: TextAreaFieldProps) {
  return (
    <FieldShell label={label}>
      <textarea
        value={value ?? ""}
        rows={rows}
        onChange={(e) =>
          onChange(e.currentTarget.value === "" ? undefined : e.currentTarget.value)
        }
        className="w-full px-2 py-1 rounded border border-[var(--color-border)] bg-white text-sm font-sans"
      />
    </FieldShell>
  );
}

interface NumberFieldProps {
  label: string;
  value: number | null | undefined;
  onChange: (v: number | undefined) => void;
  min?: number;
  max?: number;
  step?: number;
  hint?: string;
}

export function NumberField({
  label,
  value,
  onChange,
  min,
  max,
  step,
  hint,
}: NumberFieldProps) {
  return (
    <FieldShell label={label} hint={hint}>
      <input
        type="number"
        value={value ?? ""}
        min={min}
        max={max}
        step={step ?? "any"}
        onChange={(e) => {
          const raw = e.currentTarget.value;
          if (raw === "") return onChange(undefined);
          const n = Number(raw);
          onChange(Number.isFinite(n) ? n : undefined);
        }}
        className="w-full px-2 py-1 rounded border border-[var(--color-border)] bg-white text-sm font-mono"
      />
    </FieldShell>
  );
}

interface BoolFieldProps {
  label: string;
  value: boolean | undefined;
  onChange: (v: boolean | undefined) => void;
}

export function BoolField({ label, value, onChange }: BoolFieldProps) {
  return (
    <FieldShell label={label} hint="unset = field omitted from YAML">
      <select
        value={value === undefined ? "" : value ? "true" : "false"}
        onChange={(e) => {
          const v = e.currentTarget.value;
          onChange(v === "" ? undefined : v === "true");
        }}
        className="w-full px-2 py-1 rounded border border-[var(--color-border)] bg-white text-sm"
      >
        <option value="">unset</option>
        <option value="true">true</option>
        <option value="false">false</option>
      </select>
    </FieldShell>
  );
}

interface EnumSelectProps<T extends string> {
  label: string;
  value: T | undefined;
  options: readonly T[];
  onChange: (v: T | undefined) => void;
  required?: boolean;
  allowEmpty?: boolean;
}

export function EnumSelect<T extends string>({
  label,
  value,
  options,
  onChange,
  required,
  allowEmpty = true,
}: EnumSelectProps<T>) {
  return (
    <FieldShell label={label} required={required}>
      <select
        value={value ?? ""}
        onChange={(e) => {
          const v = e.currentTarget.value;
          onChange(v === "" ? undefined : (v as T));
        }}
        className="w-full px-2 py-1 rounded border border-[var(--color-border)] bg-white text-sm"
      >
        {allowEmpty && <option value="">—</option>}
        {options.map((o) => (
          <option key={o} value={o}>
            {o}
          </option>
        ))}
      </select>
    </FieldShell>
  );
}

interface StringListFieldProps {
  label: string;
  value: string[] | undefined;
  onChange: (v: string[] | undefined) => void;
  hint?: string;
}

export function StringListField({
  label,
  value,
  onChange,
  hint = "one entry per line",
}: StringListFieldProps) {
  const text = (value ?? []).join("\n");
  return (
    <FieldShell label={label} hint={hint}>
      <textarea
        value={text}
        rows={3}
        onChange={(e) => {
          const lines = e.currentTarget.value
            .split("\n")
            .map((l) => l.trim())
            .filter((l) => l.length > 0);
          onChange(lines.length === 0 ? undefined : lines);
        }}
        className="w-full px-2 py-1 rounded border border-[var(--color-border)] bg-white text-sm font-mono"
      />
    </FieldShell>
  );
}

interface CheckboxSetFieldProps {
  label: string;
  value: string[] | undefined;
  options: readonly string[];
  onChange: (v: string[] | undefined) => void;
  hint?: string;
}

export function CheckboxSetField({
  label,
  value,
  options,
  onChange,
  hint,
}: CheckboxSetFieldProps) {
  const selected = new Set(value ?? []);
  const toggle = (opt: string) => {
    const next = new Set(selected);
    next.has(opt) ? next.delete(opt) : next.add(opt);
    const arr = options.filter((o) => next.has(o));
    onChange(arr.length === 0 ? undefined : arr);
  };
  return (
    <FieldShell label={label} hint={hint}>
      <div className="flex flex-wrap gap-x-4 gap-y-1 py-1">
        {options.map((opt) => (
          <label key={opt} className="flex items-center gap-1.5 text-sm cursor-pointer">
            <input
              type="checkbox"
              checked={selected.has(opt)}
              onChange={() => toggle(opt)}
            />
            <span className="font-mono">{opt}</span>
          </label>
        ))}
      </div>
    </FieldShell>
  );
}

interface KeyValueRow {
  key: string;
  value: number;
}

interface KeyValueNumberFieldProps {
  label: string;
  value: Record<string, number> | undefined;
  onChange: (v: Record<string, number> | undefined) => void;
  hint?: string;
}

export function KeyValueNumberField({
  label,
  value,
  onChange,
  hint = "nutrient → ratio (0.0–1.0)",
}: KeyValueNumberFieldProps) {
  const rows: KeyValueRow[] = Object.entries(value ?? {}).map(([key, v]) => ({
    key,
    value: v,
  }));

  const emit = (next: KeyValueRow[]) => {
    const obj: Record<string, number> = {};
    for (const r of next) {
      if (r.key.trim()) obj[r.key.trim()] = r.value;
    }
    onChange(Object.keys(obj).length === 0 ? undefined : obj);
  };

  return (
    <FieldShell label={label} hint={hint}>
      <div className="space-y-1">
        {rows.map((row, i) => (
          <div key={i} className="flex gap-1">
            <input
              type="text"
              value={row.key}
              placeholder="nutrient key"
              onChange={(e) =>
                emit(
                  rows.map((r, j) =>
                    j === i ? { ...r, key: e.currentTarget.value } : r,
                  ),
                )
              }
              className="flex-1 px-2 py-1 rounded border border-[var(--color-border)] bg-white text-sm font-mono"
            />
            <input
              type="number"
              value={Number.isFinite(row.value) ? row.value : ""}
              min={0}
              max={1}
              step="any"
              onChange={(e) =>
                emit(
                  rows.map((r, j) =>
                    j === i
                      ? { ...r, value: Number(e.currentTarget.value) }
                      : r,
                  ),
                )
              }
              className="w-24 px-2 py-1 rounded border border-[var(--color-border)] bg-white text-sm font-mono"
            />
            <button
              type="button"
              onClick={() => emit(rows.filter((_, j) => j !== i))}
              className="px-2 py-1 rounded border border-[var(--color-border)] text-xs"
              title="Remove"
            >
              ✕
            </button>
          </div>
        ))}
        <button
          type="button"
          onClick={() => emit([...rows, { key: "", value: 1.0 }])}
          className="px-2 py-1 rounded border border-[var(--color-border)] text-xs"
        >
          + Add
        </button>
      </div>
    </FieldShell>
  );
}
