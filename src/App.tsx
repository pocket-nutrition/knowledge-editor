import { NavLink, Navigate, Route, Routes } from "react-router-dom";
import { IngredientsList } from "./routes/IngredientsList";
import { IngredientDetail } from "./routes/IngredientDetail";
import { Dashboard } from "./routes/Dashboard";
import { StatusBar } from "./components/StatusBar";

function NavItem({ to, label }: { to: string; label: string }) {
  return (
    <NavLink
      to={to}
      className={({ isActive }) =>
        `px-3 py-1.5 rounded-md text-sm transition ${
          isActive
            ? "bg-[var(--color-accent)] text-white"
            : "text-[var(--color-text-muted)] hover:bg-[var(--color-surface-2)] hover:text-[var(--color-text)]"
        }`
      }
    >
      {label}
    </NavLink>
  );
}

function App() {
  return (
    <div className="h-full flex flex-col">
      <header className="border-b border-[var(--color-border)] bg-[var(--color-surface)]">
        <div className="px-4 py-2 flex items-center gap-4">
          <div className="font-semibold tracking-tight">
            pocket-nutrition-knowledge-editor
          </div>
          <nav className="flex items-center gap-1">
            <NavItem to="/dashboard" label="Dashboard" />
            <NavItem to="/ingredients" label="Ingredients" />
          </nav>
          <div className="ml-auto">
            <StatusBar />
          </div>
        </div>
      </header>
      <main className="flex-1 overflow-auto">
        <Routes>
          <Route path="/" element={<Navigate to="/dashboard" replace />} />
          <Route path="/dashboard" element={<Dashboard />} />
          <Route path="/ingredients" element={<IngredientsList />} />
          <Route path="/ingredients/:id" element={<IngredientDetail />} />
        </Routes>
      </main>
    </div>
  );
}

export default App;
