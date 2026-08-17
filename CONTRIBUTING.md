# Contributing to Pocket Nutrition Knowledge Editor

Thanks for your interest in contributing. This is a local-only desktop tool (Tauri 2 +
React 19 + TypeScript) for editing the `pocket-nutrition/knowledge` YAML catalogue — there
is no server deployment involved, which keeps the contribution loop simple.

## Development Setup

### Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Node.js | 18+ | `brew install node` or [nodejs.org](https://nodejs.org) |
| Rust + Cargo | stable | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |

Verify Cargo is on your `PATH`:

```bash
cargo --version
```

If that fails with something like `command not found: cargo`, source the Rustup env file and
retry:

```bash
. "$HOME/.cargo/env"
cargo --version
```

Add the `source` line to your shell profile (`~/.zshrc` or `~/.bashrc`) so it persists across
sessions.

### Install and run

```bash
git clone https://github.com/pocket-nutrition/knowledge-editor.git
cd knowledge-editor
npm install
npm run tauri:dev
```

`npm run tauri:dev` invokes `tauri dev`, which in turn runs `npm run dev` (Vite, port 1420) for
the frontend and compiles the Rust backend in `src-tauri/` — no separate steps are needed.

Other useful scripts (see `package.json`):

```bash
npm run dev            # Vite dev server only (frontend, no Tauri window)
npm run build           # tsc + vite build (frontend production bundle)
npm run tauri:build     # Full native app bundle
```

There is no configured lint or test script in `package.json` at present. Rust-side checks:

```bash
cd src-tauri
cargo check    # Fast compile check
cargo build    # Full build
```

## How the App Finds the Knowledge Repo

The app needs a local clone of [`pocket-nutrition/knowledge`](https://github.com/pocket-nutrition/knowledge)
on disk (a directory containing `ingredients/`, `source_mappings/`, `cooking_relationships/`,
and `schemas/` subfolders). It is resolved in `src-tauri/src/repo.rs` with the following
priority order, first match wins:

1. **`POCKET_NUTRITION_KNOWLEDGE_PATH` environment variable** — set this for local development
   or CI, e.g.:
   ```bash
   POCKET_NUTRITION_KNOWLEDGE_PATH=/path/to/knowledge npm run tauri:dev
   ```
2. **Config file** — `repo_path` key in a `config.toml` under the OS config directory:
   - macOS: `~/Library/Application Support/pocket-nutrition-knowledge-editor/config.toml`
   - Linux: `~/.config/pocket-nutrition-knowledge-editor/config.toml`

   ```toml
   repo_path = "/path/to/knowledge"
   ```

   The same config file can also set `community_url` and `community_reload_token`, used by the
   "trigger reload" feature that calls a local `pocket-nutrition/community` instance
   (defaults to `http://localhost:8001`).
3. **Hard default** — `~/dev/pocketnutrition/knowledge`, if neither of the
   above is set.

If the resolved path doesn't exist, or is missing one of the four required subdirectories, the
app will fail to start with a clear error naming the missing path — this is intentional and not
a bug.

### Git behavior

The app's built-in git integration (`git_status` command) is **read-only** — it never commits
or pushes on your behalf. After editing ingredients through the UI, you are expected to review
and commit the changes to `pocket-nutrition/knowledge` yourself using your normal `git` workflow.

## Branch and PR Workflow

1. Fork the repository and create a feature branch off `main`:
   ```bash
   git checkout -b feat/short-description
   ```
2. Make your changes, keeping commits focused and descriptive.
3. Verify the app still builds:
   ```bash
   npm run build
   (cd src-tauri && cargo check)
   ```
4. Commit using a conventional prefix (`feat:`, `fix:`, `refactor:`, `docs:`, `chore:`, …):
   ```bash
   git commit -m "feat: add bulk mapping review view"
   ```
5. Push your branch and open a pull request against `main`. Describe what changed and why,
   and include screenshots for UI changes.
6. Be responsive to review feedback — small, focused PRs are easier to review and merge.

## Code Style

- **TypeScript/React**: components under `src/components/`, routes under `src/routes/`,
  shared logic under `src/lib/`. No linter is currently configured — match the existing style
  in nearby files.
- **Rust**: run `cargo fmt` before committing and `cargo clippy` to catch common issues, even
  though neither is currently enforced in CI.
- Keep files focused; prefer several small modules over one large one.

## Reporting Issues

- Search existing issues before opening a new one.
- Use the provided issue templates (bug report / feature request) under
  `.github/ISSUE_TEMPLATE/`.
- For bugs, include your OS, Node/Rust versions, and exact steps to reproduce.
