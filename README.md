# Pocket Nutrition Knowledge Editor

Desktop GUI (Tauri 2 + React + TypeScript) for browsing and editing the `pocket-nutrition/knowledge` YAML ingredient catalogue.

This is a **local-only** desktop tool — it is not deployed anywhere and has no server component.
It reads and writes YAML files directly in a local clone of
[`pocket-nutrition/knowledge`](https://github.com/pocket-nutrition/knowledge); changes must be
committed and pushed from that clone using your normal `git` workflow (this app's git
integration is read-only, showing status only).

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Node.js | 18+ | `brew install node` |
| Rust + Cargo | stable | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |

### Make sure Cargo is in your PATH

Rustup installs Cargo to `~/.cargo/bin`. Add it to your shell profile if it isn't there:

```bash
# ~/.zshrc (or ~/.bashrc)
. "$HOME/.cargo/env"
```

Then reload: `source ~/.zshrc`

Verify with: `cargo --version`

> **Common error:** `failed to run 'cargo metadata'… No such file or directory`
> This means `cargo` is not in the PATH. Apply the fix above and retry.

## Install dependencies

```bash
git clone https://github.com/pocket-nutrition/knowledge-editor.git
cd knowledge-editor
npm install
```

## Point the app at a knowledge repo clone

The app needs a local clone of `pocket-nutrition/knowledge` on disk. It looks for one in this
order (first match wins):

1. `POCKET_NUTRITION_KNOWLEDGE_PATH` environment variable
2. `repo_path` in a config file:
   - macOS: `~/Library/Application Support/pocket-nutrition-knowledge-editor/config.toml`
   - Linux: `~/.config/pocket-nutrition-knowledge-editor/config.toml`
3. Default: `~/dev/pocketnutrition/knowledge`

```bash
POCKET_NUTRITION_KNOWLEDGE_PATH=/path/to/knowledge npm run tauri:dev
```

If the resolved path is missing or doesn't contain `ingredients/`, `source_mappings/`,
`cooking_relationships/`, and `schemas/` subfolders, the app will fail to start with an error
naming the problem path.

## Run in development

```bash
npm run tauri:dev
```

## Build for production

```bash
npm run tauri:build
```

## Stack

- **Tauri 2** — native shell
- **React 19** + **TypeScript**
- **Vite 7**
- **Tailwind CSS 4**
- **Zustand** — state management
- **React Router 7**

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development workflow, branch/PR conventions, and
details on how the knowledge repo path is resolved.

## License

AGPL-3.0 — see [LICENSE](LICENSE).
