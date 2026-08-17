import { useEffect, useState } from "react";
import { api } from "../lib/tauri";
import type { DashboardStats } from "../lib/types";

export function Dashboard() {
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    api
      .dashboardStats()
      .then(setStats)
      .catch((e) => setErr(String(e)));
  }, []);

  if (err) {
    return (
      <div className="p-6 text-[var(--color-danger)]">
        Failed to load stats: {err}
      </div>
    );
  }

  if (!stats) {
    return (
      <div className="p-6 text-[var(--color-text-muted)]">Loading…</div>
    );
  }

  return (
    <div className="p-6 max-w-5xl mx-auto space-y-8">
      <div className="grid grid-cols-3 gap-4">
        <Stat label="Ingredients" value={stats.total_ingredients} />
        <Stat label="Mappings" value={stats.total_mappings} />
        <Stat label="Relationships" value={stats.total_relationships} />
      </div>

      <section>
        <h2 className="text-sm font-semibold tracking-wide uppercase text-[var(--color-text-muted)] mb-2">
          By food group
        </h2>
        <div className="grid grid-cols-4 gap-2">
          {Object.entries(stats.by_food_group)
            .sort((a, b) => b[1] - a[1])
            .map(([group, count]) => (
              <div
                key={group}
                className="px-3 py-2 rounded border border-[var(--color-border)] bg-[var(--color-surface-2)] text-sm flex justify-between"
              >
                <span>{group}</span>
                <span className="font-mono">{count}</span>
              </div>
            ))}
        </div>
      </section>

      <section>
        <h2 className="text-sm font-semibold tracking-wide uppercase text-[var(--color-text-muted)] mb-2">
          Mappings progress
        </h2>
        <table className="w-full text-sm">
          <thead className="text-left text-[var(--color-text-muted)]">
            <tr>
              <th className="py-1">Source</th>
              <th>Total</th>
              <th>Linked</th>
              <th>Composite</th>
              <th>Unlinked</th>
              <th>Progress</th>
            </tr>
          </thead>
          <tbody>
            {Object.entries(stats.mappings_by_source).map(([source, s]) => {
              const decided = s.linked + s.composite;
              const pct = s.total ? Math.round((decided / s.total) * 100) : 0;
              return (
                <tr key={source} className="border-t border-[var(--color-border)]">
                  <td className="py-1.5 font-mono">{source}</td>
                  <td>{s.total}</td>
                  <td className="text-[var(--color-success)]">{s.linked}</td>
                  <td className="text-[var(--color-text-muted)]">
                    {s.composite}
                  </td>
                  <td className="text-[var(--color-warning)]">{s.unlinked}</td>
                  <td className="font-mono">{pct}%</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </section>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="p-4 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-2)]">
      <div className="text-xs uppercase tracking-wide text-[var(--color-text-muted)]">
        {label}
      </div>
      <div className="text-3xl font-semibold mt-1 font-mono">
        {value.toLocaleString("fr-FR")}
      </div>
    </div>
  );
}
