import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "../../lib/tauri";
import { slugify } from "../../lib/slug";
import { TextField } from "./fields";

interface Props {
  sourceId: string;
  sourceNameEn: string;
  onClose: () => void;
}

const ID_PATTERN = /^[a-z0-9_]{1,128}$/;

export function CloneIngredientPanel({
  sourceId,
  sourceNameEn,
  onClose,
}: Props) {
  const navigate = useNavigate();
  const [nameEn, setNameEn] = useState(`${sourceNameEn} copy`);
  const [id, setId] = useState(slugify(`${sourceNameEn} copy`));
  const [idTouched, setIdTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const onNameChange = (v: string | undefined) => {
    const next = v ?? "";
    setNameEn(next);
    // Keep the id in sync with the name until the user edits it directly.
    if (!idTouched) setId(slugify(next));
  };

  const idValid = ID_PATTERN.test(id);
  const canSubmit = !busy && nameEn.trim().length > 0 && idValid;

  const submit = async () => {
    if (!canSubmit) return;
    setBusy(true);
    setError(null);
    try {
      const created = await api.cloneIngredient(sourceId, id, nameEn.trim());
      navigate(`/ingredients/${created.id}`);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  return (
    <div className="mt-3 border border-[var(--color-border)] rounded-lg p-4 space-y-3 bg-[var(--color-surface)]">
      <div className="text-sm font-semibold">Clone “{sourceNameEn}”</div>
      <p className="text-xs text-[var(--color-text-muted)]">
        Copies every characteristic except the id and English name. No source
        mappings are carried over — attach new ones afterward to give the clone
        different nutrition.
      </p>

      <div className="grid grid-cols-2 gap-x-6 gap-y-3">
        <TextField
          label="New name (EN)"
          value={nameEn}
          onChange={onNameChange}
          required
          placeholder="egg copy"
        />
        <TextField
          label="New ID"
          value={id}
          onChange={(v) => {
            setIdTouched(true);
            setId((v ?? "").toLowerCase());
          }}
          required
          placeholder="egg_copy"
        />
      </div>

      {!idValid && id.length > 0 && (
        <div className="text-xs text-[var(--color-danger)]">
          ID must match a–z, 0–9 and underscore only (1–128 chars).
        </div>
      )}
      {error && (
        <div className="text-xs text-[var(--color-danger)] break-words">
          {error}
        </div>
      )}

      <div className="flex items-center justify-end gap-2">
        <button
          type="button"
          onClick={onClose}
          disabled={busy}
          className="px-3 py-1.5 rounded border border-[var(--color-border)] text-sm disabled:opacity-50"
        >
          Cancel
        </button>
        <button
          type="button"
          onClick={submit}
          disabled={!canSubmit}
          className="px-3 py-1.5 rounded bg-[var(--color-accent)] text-white text-sm disabled:opacity-50"
        >
          {busy ? "Cloning…" : "Clone"}
        </button>
      </div>
    </div>
  );
}
