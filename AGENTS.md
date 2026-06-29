# Codeoba Agent Instructions

Welcome! You are an AI coding assistant working on the Tauri migration of **Codeoba**—a platform-agnostic, zero-external-dependency, 100% local search application that indexes, monitors, and searches conversation transcripts across Claude Code, Google Antigravity, Cursor, OpenAI Codex, Aider, and GitHub Copilot.

This file acts as the primary repository context and instruction guide for Tauri development. Read this first to align with the codebase.

---

## 📖 Documentation & Workspace Guidelines

To ensure the project context remains accurate:
1. **Synchronized Updates:** When code structures, design decisions, source adapters, or file paths change, you must update the relevant codebase documentation (including this file `AGENTS.md`, the root `README.md`, and architectural files under `docs/`).
2. **Definition of Done:** A task, refactoring, or feature implementation is not complete until all corresponding documentation has been updated to reflect the new state of the codebase.
3. **No Automatic Git Staging/Commits:** By default, never stage (`git add`) or commit (`git commit`) changes unless explicitly requested or prompted by the user.
4. **Relative Pathing Requirement:** Always write file paths relative to the folder they are in (e.g., `./README.md` or `./src-tauri/`). Never document absolute file paths or paths outside of the repository.
5. **Plan Synchronization:** Any time a CLI command, parameter, file path, or configuration flag changes or is corrected during implementation, you must immediately propagate that change to the local `implementation_plan.md` in the system app data directory.
6. **Test Verification:** Before completing any task, code modifications, or refactoring, you MUST run all unit and integration tests locally (e.g., `npm run build`, `npm run test`, and `cargo test --manifest-path src-tauri/Cargo.toml` if Rust core code changes are made) to ensure all tests pass and no regressions are introduced.
7. **Conventional Commits:** All commits MUST follow the Conventional Commits specification (https://www.conventionalcommits.org) using standard prefixes (e.g., `feat:`, `fix:`, `docs:`, `chore:`).

---

## 🏗️ Codebase Directory Map

- **`src/` (Frontend React UI)**
  - `main.tsx`: App rendering and React root element bootstrap.
  - `App.tsx`: App layout coordinator (managing navigation and pane displays).
  - `index.css`: Tailwind CSS entry stylesheet introducing variables.
  - `components/`: Reusable UI elements (dialog box widgets, buttons, status indicators).
  - `panels/`: Complex panels:
    * `Sidebar.tsx`: Search inputs, source selectors, sorting dropdowns, and index thread lists.
    * `DetailPane.tsx`: Conversation dialogue display, Markdown parsing, metadata tags, and copy buttons.
    * `SettingsDialog.tsx`: General settings, source path managers, permissions console.
  - `hooks/`: React state and lifecycle hooks (`useSearch`, `useWatcher`, `useAuth`).
  - `services/`: Bridges to call Tauri commands via TS functions (`tauriBridge.ts`).

- **`src-tauri/` (Backend Rust Core)**
  - `Cargo.toml`: Package dependencies (tauri, serde, rusqlite, notify, keyring, ort, wasmtime).
  - `src/main.rs`: Minimal entry point that boots the library runner.
  - `src/lib.rs`: Tauri builder, setup hooks, and deep link integrations.
  - `src/commands.rs`: Rust command handlers exposed via IPC to the React frontend.
  - `src/models.rs`: Rust structs mapping unified types (`Session`, `Turn`, `SessionSummary`).
  - `src/parsers/`: Log adapters parsing files to models:
    * `claude.rs`: JSONL stream parser.
    * `cursor.rs`: SQLite workspace parser.
    * `antigravity.rs`: Antigravity JSONL parser.
    * `aider.rs`: Aider Markdown history parser.
    * `copilot.rs` & `codex.rs`: Stream log event deserializers.
  - `src/search/`: Vector ONNX and lexical search logic.
  - `src/tokenizer.rs`: Offline BPE-based token count estimator (family scales, Hugging Face config loader).
  - `src/watcher.rs`: Native OS filesystem file monitoring.
  - `src/keyring.rs`: Keychain and Credential Manager secure integrations.
  - `src/premium/`: Signed WASM premium code verification and runner.

- **`docs/` (Architectural Documentation)**
  - `tokenization_calibration.md`: Hybrid Offline/Online tokenization calibration & simulation system design.

---

## 🎨 UI Style Guidelines & Constraints (React + Tailwind CSS)

When modifying the frontend web components, adhere to these styling guidelines:

1. **Dynamic Color Theme Styling**:
   - Theme variables (e.g. background, surface, borders, highlight accents, text) are defined as CSS Custom Properties (variables) in `src/index.css`.
   - The React app loads the theme selection from the backend on startup and injects it as a CSS class or data attribute on the `<html>` or `<body>` element (e.g. `data-theme="nordic-frost"`).
   - Tailwind styles must use these semantic variable names (e.g., `bg-background`, `border-border`, `text-primary`, `text-accent-cyan`).
   - The 8 custom themes are: Obsidian, Nordic Frost, Emerald Forest, Sunset Copper, Royal Amethyst, Dracula, Cyberpunk Neon, and Monochrome Slate.

2. **Casing & Naming**:
   - Never display uppercase-only labels like "USER" or "ASSISTANT" in the transcripts. Use capitalized words (e.g. "User", "Assistant").

3. **macOS Window Layout & Spacing**:
   - Clear a `76px` top-left padding/margin area (`pt-[76px]` or `mt-[76px]`) on macOS to avoid overlapping the macOS transparent titlebar window controls.
   - Maintain breadcrumb navigation: `Workspace Name / Active Session Title`.

4. **Markdown Rendering & Code Highlighting**:
   - Use `marked` on the frontend for rendering transcripts.
   - Syntactically highlight code snippets using `prismjs` or `shiki` within React code block layouts.
   - Handle markdown links inside chat bubbles by attaching clickable callback events that securely verify target paths with the backend before opening them.

---

## ⚙️ Core Architecture Patterns

1. **Cursor State WAL & Orphan Filtering**:
   - SQLite connections to Cursor use read-only WAL mode (`mode=ro` in rusqlite) to query files without creating lock conflicts.
   - Skip database rows that are not listed in the active workspace's local `allComposers` list to automatically hide deleted sessions.

2. **Directory Watcher (notify crate)**:
   - Use Rust's `notify` crate to receive native OS file events.
   - Keep event-driven file monitoring filtered to specific target log extensions (`.jsonl`, `.md`, `.vscdb`) to prevent index-write loops on source codes or builds.

3. **Local ONNX-based Semantic Search**:
   - The `all-MiniLM-L6-v2` transformer model is bundled/cached locally in `~/.codeoba/models/`.
   - Execution is run on the Rust backend using the `ort` crate and Hugging Face `tokenizers` crate.
   - Run tokenization and inference asynchronously in a background thread pool (e.g., using `tokio` or `rayon`) to prevent blocking the UI thread.
   - Store computed vectors locally in `~/.codeoba/cache/embeddings_cache.json` using the keyring encryption key.

4. **Dynamic WASM Premium Module Execution (`premium.wasm`)**:
   - **Verification**: The client downloads the signed module and verifies its signature against the public KMS release key using `ed25519-dalek`.
   - **Execution**: The Rust backend executes the WASM binary using `wasmtime` or `wasmer` inside the backend process.
   - **Data Transfer**: Exchange session details and summaries between Rust and WASM by passing lightweight JSON serialization strings.

5. **Keychain Credential Storage**:
   - Retrieve and save credentials (tokens, billing JWTs, encryption key) via the Rust `keyring` crate.
   - Automatically bypass keychain prompts in non-production environments to avoid authorization dialogs on unsigned builds (toggled via `-Dcodeoba.no.keyring` equivalent or checking dev build compilation). Use a local user-restricted JSON config file fallback.

6. **Ecosystem Sync & Deep Linking**:
   - Implement deep-linking callback hooks on the custom protocol `codeoba://callback` using the `tauri-plugin-deeplink` plugin, replacing the JDK HTTP loopback server.

7. **Auto-Updates & Deployment Pipeline**:
   - **Built-in Updater**: Utilizes Tauri's built-in updater (`tauri-plugin-updater`) for release checks and installer execution.
   - **Two-Tier Key Signing**: Uses development key pairs for pushes to `main` (staging builds) and production key pairs for tag pushes (`v*`). Private key variables are `CODEOBA_TAURI_UPDATE_PRIVATE_KEY_DEV` and `_PROD`. Public key variables are `CODEOBA_TAURI_UPDATE_PUBLIC_KEY_DEV` and `_PROD`.
   - **Version Suffixing**: The `./scripts/sync-version.cjs` script configures the target version:
     * Staging (pushes to `main`): appends `-<build_number>` suffix (e.g. `0.1.0-123`) and sets the update endpoint to `https://dev.codeoba.com/api/update`.
     * Production (tag pushes): uses clean semver version and sets the update endpoint to `https://codeoba.com/api/update`.
   - **Manifest Merging**: The `./scripts/merge-updater-manifests.cjs` script merges platform-specific `latest.json` files generated by matrix runners into a single, unified manifest and rewrites asset URLs.
   - **Staging Pre-Release Pruning**: The `./scripts/prune-dev-releases.cjs` script programmatically deletes previous dev pre-releases and tag references using the GitHub CLI to prevent release list spam. Running it locally with the `--local` flag (e.g., `node scripts/prune-dev-releases.cjs --local`) will scan and delete matching pre-release tags in the local Git repository instead.
   - **Staging Update Resolver**: Staging clients query `dev.codeoba.com/api/update`, which dynamically resolves the latest dev pre-release update manifest using the `CODEOBA_TAURI_LATEST_JSON_URL=DYNAMIC_DEV` backend configuration.

8. **Store Screenshot Generator & Mock Mode**:
   - Intercept log folders parsing and load canned mock datasets (`canned_apple.json` or `canned_microsoft.json`) if `--store microsoft` or `--store apple` flags are passed to the app cli entrypoint.

---

## 🛠️ Common Cargo & NPM Development Commands

- Install frontend packages: `npm install`
- Launch Tauri application in hot-reloading dev environment: `npm run dev` or `npm run tauri dev`
- Run Rust backend unit tests: `cargo test --manifest-path src-tauri/Cargo.toml`
- Compile production packages/installers locally (without updater signing): `npm run build:local`
