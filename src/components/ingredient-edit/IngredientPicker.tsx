import { useEffect, useRef, useState } from "react";
import { api } from "../../lib/tauri";
import type { IngredientSummary } from "../../lib/types";

interface Props {
  excludeId: string;
  onSelect: (ingredientId: string) => void;
  busy?: boolean;
}

export function IngredientPicker({ excludeId, onSelect, busy }: Props) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<IngredientSummary[]>([]);
  const [error, setError] = useState<string | null>(null);
  const debounceRef = useRef<number | null>(null);

  useEffect(() => {
    if (debounceRef.current !== null) window.clearTimeout(debounceRef.current);
    debounceRef.current = window.setTimeout(async () => {
      try {
        const res = await api.listIngredients({ search: query, limit: 20 });
        setResults(res.items.filter((i) => i.id !== excludeId));
        setError(null);
      } catch (e) {
        setError(String(e));
      }
    }, 300);
    return () => {
      if (debounceRef.current !== null) window.clearTimeout(debounceRef.current);
    };
  }, [query, excludeId]);

  return (
    <div className="mt-2 border border-[var(--color-border)] rounded-lg p-3 space-y-2 bg-[var(--color-surface)]">
      <input
        type="text"
        placeholder="Search target ingredient by name or ID…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        className="w-full text-xs px-2 py-1 rounded border border-[var(--color-border)] bg-[var(--color-surface)]"
        autoFocus
      />

      {error && (
        <div className="text-xs text-[var(--color-danger)] px-1">{error}</div>
      )}

      {results.length === 0 ? (
        <div className="text-xs text-[var(--color-text-muted)] px-1">
          {query
            ? "No ingredients match your search."
            : "Type to search ingredients."}
        </div>
      ) : (
        <ul className="divide-y divide-[var(--color-border)] border border-[var(--color-border)] rounded">
          {results.map((i) => (
            <li
              key={i.id}
              className="px-3 py-1.5 text-sm flex items-center gap-3"
            >
              <div className="flex-1 min-w-0">
                <div className="truncate">
                  <span className="font-mono text-xs text-[var(--color-text-muted)] mr-2">
                    {i.id}
                  </span>
                  {i.name_en}
                </div>
                {i.name_fr && (
                  <div className="text-xs text-[var(--color-text-muted)] truncate">
                    {i.name_fr}
                  </div>
                )}
              </div>
              <button
                type="button"
                onClick={() => onSelect(i.id)}
                disabled={busy}
                className="shrink-0 px-2 py-0.5 text-xs rounded border border-[var(--color-accent)] text-[var(--color-accent)] hover:bg-blue-50 disabled:opacity-50"
              >
                Transfer here
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
