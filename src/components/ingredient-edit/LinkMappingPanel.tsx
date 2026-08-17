import { useEffect, useRef, useState } from "react";
import { api } from "../../lib/tauri";
import type { Source, SourceMapping } from "../../lib/types";

const ALL_SOURCES: Source[] = [
  "ciqual",
  "mccance",
  "usda",
  "usda_foundation",
  "usda_sr",
  "reference",
];

interface Props {
  ingredientId: string;
  onLinked: () => void;
}

export function LinkMappingPanel({ ingredientId, onLinked }: Props) {
  const [sourceFilter, setSourceFilter] = useState<Source | null>(null);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SourceMapping[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const debounceRef = useRef<number | null>(null);

  useEffect(() => {
    if (debounceRef.current !== null) window.clearTimeout(debounceRef.current);
    debounceRef.current = window.setTimeout(async () => {
      try {
        const items = await api.searchUnlinkedMappings(sourceFilter, query, 20);
        setResults(items);
        setError(null);
      } catch (e) {
        setError(String(e));
      }
    }, 300);
    return () => {
      if (debounceRef.current !== null) window.clearTimeout(debounceRef.current);
    };
  }, [sourceFilter, query]);

  const handleLink = async (m: SourceMapping) => {
    const key = `${m.source}-${m.source_id}`;
    setBusy(key);
    setError(null);
    try {
      await api.linkMapping(m.source, m.source_id, ingredientId);
      setResults((prev) =>
        prev.filter((r) => !(r.source === m.source && r.source_id === m.source_id)),
      );
      onLinked();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="mt-3 border border-[var(--color-border)] rounded-lg p-3 space-y-2 bg-[var(--color-surface)]">
      <div className="flex gap-2">
        <select
          value={sourceFilter ?? ""}
          onChange={(e) =>
            setSourceFilter(e.target.value ? (e.target.value as Source) : null)
          }
          className="text-xs px-2 py-1 rounded border border-[var(--color-border)] bg-[var(--color-surface)] shrink-0"
        >
          <option value="">All sources</option>
          {ALL_SOURCES.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
        <input
          type="text"
          placeholder="Search by label or source ID…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="flex-1 text-xs px-2 py-1 rounded border border-[var(--color-border)] bg-[var(--color-surface)]"
          autoFocus
        />
      </div>

      {error && (
        <div className="text-xs text-[var(--color-danger)] px-1">{error}</div>
      )}

      {results.length === 0 ? (
        <div className="text-xs text-[var(--color-text-muted)] px-1">
          {query || sourceFilter
            ? "No unlinked mappings match your search."
            : "Type to search unlinked mappings."}
        </div>
      ) : (
        <ul className="divide-y divide-[var(--color-border)] border border-[var(--color-border)] rounded">
          {results.map((m) => {
            const key = `${m.source}-${m.source_id}`;
            return (
              <li
                key={key}
                className="px-3 py-1.5 text-sm flex items-center gap-3"
              >
                <div className="flex-1 min-w-0">
                  <div className="truncate">
                    <span className="font-mono text-xs text-[var(--color-text-muted)] mr-2">
                      {m.source}#{m.source_id}
                    </span>
                    {m.label}
                  </div>
                  {m.confidence != null && (
                    <div className="text-xs text-[var(--color-text-muted)]">
                      {m.match_method ?? "—"} · {m.confidence.toFixed(2)}
                    </div>
                  )}
                </div>
                <button
                  type="button"
                  onClick={() => handleLink(m)}
                  disabled={busy === key}
                  className="shrink-0 px-2 py-0.5 text-xs rounded border border-[var(--color-accent)] text-[var(--color-accent)] hover:bg-blue-50 disabled:opacity-50"
                >
                  Link
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
