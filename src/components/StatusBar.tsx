import { useEffect, useState } from "react";
import { api } from "../lib/tauri";

interface GitStatus {
  modified: string[];
  untracked: string[];
}

export function StatusBar() {
  const [status, setStatus] = useState<GitStatus | null>(null);
  const [reloading, setReloading] = useState(false);
  const [reloadMsg, setReloadMsg] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      try {
        const s = await api.gitStatus();
        if (!cancelled) setStatus(s);
      } catch {
        // Backend not ready yet — ignore.
      }
    };
    tick();
    const id = setInterval(tick, 5000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  const modifiedCount =
    (status?.modified.length ?? 0) + (status?.untracked.length ?? 0);

  const onReload = async () => {
    setReloading(true);
    setReloadMsg(null);
    try {
      const r = await api.triggerCommunityReload();
      setReloadMsg(r.ok ? "Reloaded" : r.detail ?? "Failed");
    } catch (e) {
      setReloadMsg(String(e));
    } finally {
      setReloading(false);
      setTimeout(() => setReloadMsg(null), 3000);
    }
  };

  return (
    <div className="flex items-center gap-3 text-xs">
      <div className="flex items-center gap-1.5 text-[var(--color-text-muted)]">
        <span
          className={
            modifiedCount > 0
              ? "inline-block w-2 h-2 rounded-full bg-[var(--color-warning)]"
              : "inline-block w-2 h-2 rounded-full bg-[var(--color-success)]"
          }
        />
        {modifiedCount > 0
          ? `${modifiedCount} fichier${modifiedCount > 1 ? "s" : ""} modifié${modifiedCount > 1 ? "s" : ""}`
          : "Worktree clean"}
      </div>
      <button
        onClick={onReload}
        disabled={reloading}
        className="px-2 py-1 rounded border border-[var(--color-border)] hover:bg-[var(--color-surface-2)] disabled:opacity-50"
        title="POST /admin/reload to pocket-nutrition-community"
      >
        {reloading ? "…" : "Reload Community"}
      </button>
      {reloadMsg && (
        <span className="text-[var(--color-text-muted)]">{reloadMsg}</span>
      )}
    </div>
  );
}
