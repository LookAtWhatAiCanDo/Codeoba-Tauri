# Codeoba — Tauri Desktop App (SolidJS + Rust)

Codeoba is a platform-agnostic, zero-dependency, 100% local search application that aggregates and indexes conversation transcripts from major AI coding assistants (Claude Code, Cursor, Aider, Copilot, Codex, and Google Antigravity) into a unified desktop dashboard.

This is the Tauri-based port of the desktop application, combining a highly efficient Rust backend core with a modern SolidJS + TypeScript + Tailwind CSS frontend.

---

## 🛠️ Prerequisites

Before you run or compile the application, ensure you have the following installed on your machine:

1. **Node.js** (v18.0.0 or newer)
2. **Rust & Cargo** (v1.75.0 or newer)
   * On macOS/Linux, install via: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
   * On Windows, install using the official [Rustup installer](https://rustup.rs/)

---

## 🚀 Getting Started

Follow these steps to run the application in your local development environment:

### 1. Install Dependencies
Run the package installation command in the root of the `Codeoba-Tauri` directory:
```bash
npm install
```

### 2. Run the Development Client
Launch the hot-reloading development server and compile/run the native desktop shell wrapper:
```bash
npm run tauri dev
```
*This command starts the Vite local server on port `1420` and loads the SolidJS UI into the native operating system webview. Changes to frontend views will hot-reload instantly. Changes to the Rust backend will trigger an automatic recompilation and restart.*

---

## 📖 Command-Line Interface (CLI) Usage

Codeoba can be configured or executed directly from your terminal using custom command-line options.

### 1. Terminal Search (Headless CLI)
You can run search operations directly in your shell without spawning the desktop graphical interface:
```bash
# Perform a standard lexical (keyword-based) search
cargo run --manifest-path src-tauri/Cargo.toml -- search "your search query"

# Perform an ONNX-powered semantic vector search
cargo run --manifest-path src-tauri/Cargo.toml -- search "your search query" --semantic
```

### 2. Reset Window Geometry & Layout
If you need to reset the app's window sizes, positions, and sidebar widths back to the default 3/4 monitor dimensions (for instance, after screen configurations change or monitors are disconnected), pass the reset argument.

Because of how package managers (`npm`), the Tauri CLI, and the Rust compiler (`cargo`) chain command-line arguments, the number of double-dash (`--`) separators required varies depending on how you run the application:

*   **Using `npm run tauri dev` (requires 3 separators):**
    ```bash
    # npm consumes the first --, Tauri CLI consumes the second --, and the third -- passes the flag to the app binary
    npm run tauri dev -- -- -- --reset-window
    
    # Or with the short reset flag
    npm run tauri dev -- -- -- --reset
    ```

*   **Using `npx tauri dev` (requires 2 separators):**
    ```bash
    # Tauri CLI consumes the first --, and the second -- passes the flag to the app binary
    npx tauri dev -- -- --reset-window
    ```

*   **Using `cargo run` directly (requires 1 separator):**
    ```bash
    # Cargo consumes the -- and passes the flag to the app binary
    cargo run --manifest-path src-tauri/Cargo.toml -- --reset-window
    ```

*   **Running the compiled production binary (no separator needed):**
    ```bash
    # Arguments are passed directly to the native executable
    ./codeoba --reset-window
    ```

---

## 🧪 Testing

To execute Rust backend tests (including log parsers, search algorithms, and signature checks):
```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

---

## 📦 Building for Production

To compile and package the application locally (without requiring updater signing keys):
```bash
npm run build:local
```

For production builds (where updates are signed and verified using Minisign):
```bash
# Set private key and password before compiling
export TAURI_SIGNING_PRIVATE_KEY="your_private_key_content"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="your_key_password"
npm run tauri build
```
*Tauri compiles the Rust source code in `--release` mode, minifies frontend SolidJS assets, bundles them into the binary, and packages the installer (`.dmg` on macOS, `.msi` on Windows, `.deb` on Linux).*

---

## 🏗️ Project Architecture Map

*   **`src/` (Frontend)**: Styled with Tailwind CSS, SolidJS components manage search layouts, settings panes, markdown renderings, and UI themes.
*   **`src-tauri/` (Backend)**: Rust crate that manages directory watchers (`notify`), SQLite cache databases (`rusqlite`), credential stores (`keyring`), ONNX semantic vector inference (`ort`), and signed WASM plugin loading (`wasmtime`).

---

## 🔄 Auto-Updates & CI/CD Release Pipeline

Codeoba features secure, cryptographically-signed auto-updates hosted on GitHub Releases, powered by the Tauri v2 Updater.

### 1. Cryptographic Update Signing
By default, the repository's [tauri.conf.json](src-tauri/tauri.conf.json) points to the development/staging proxy at:
`https://dev.codeoba.com/api/update`

During tagged production builds, the CI pipeline automatically rewrites this configuration to target the production update proxy at:
`https://codeoba.com/api/update`

This proxy handles client telemetry logging and retrieves the signed `latest.json` bundle configuration directly from GitHub Releases. Update packages must be signed using a **Minisign** keypair:
*   The public key is configured inside [tauri.conf.json](src-tauri/tauri.conf.json) under `plugins.updater.pubkey`.
*   The private key is stored locally (ignored by Git) and must be provided as an environment variable (`TAURI_SIGNING_PRIVATE_KEY`) to compile updates.

#### Local/Staging Testing
By default, the updater is disabled in the repository configuration (`"active": false` under `"updater"` in `tauri.conf.json`) to prevent unwanted network checks during development. Furthermore, the public key is cleared in the repo by default (`"pubkey": ""`).

To test update checking and download progress triggers locally (against either `dev.codeoba.com` or a local mock server on `localhost`), you must follow the verification and key registration steps documented in the [Local & Staging Update Testing Guide](docs/APP_SIGNING.md#🧪-local--staging-update-testing-guide).


To sign updates locally during a build, run:
```bash
# Set private key and password (if any) before compiling
export TAURI_SIGNING_PRIVATE_KEY="your_private_key_content"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="your_key_password"
npm run tauri build
```
This generates the platform installer together with a `.sig` signature file and updater configuration block.

### 2. GitHub Actions CI/CD Pipeline
A release pipeline is configured in [.github/workflows/build-desktop.yml](.github/workflows/build-desktop.yml). It triggers automatically under two conditions:

1. **Staging / Dev Releases (Pushes to `main`)**: When code is pushed to `main`, the pipeline automatically builds and signs the packages using the staging key pair, publishes them under a unique pre-release tag (e.g. `v0.1.0-124`), and prunes previous dev pre-releases and tags to keep the release list clean. Staging clients query `dev.codeoba.com/api/update` to receive updates.
2. **Tagged Production Releases (`v*`)**: To publish a new stable production release:
   
   *   **Step 1: Bump the Version**: Synchronize the version across all project files to the target version (e.g. `0.1.3`) using the bump helper script:
       ```bash
       # Bump files locally
       npm run bump -- 0.1.3
       
       # Bump files and automatically commit the change to main
       npm run bump -- 0.1.3 --commit
       ```
       *(This consistently updates `package.json`, `package-lock.json`, `tauri.conf.json`, `Cargo.toml`, and `Cargo.lock`).*

        > [!TIP]
        > **Automatic Stash Guard:** Running the bump command with the `--commit` flag automatically stashes any ongoing local changes (`git stash`) before modifying the files, commits the version changes cleanly, and then restores your workspace (`git stash pop`) in a `finally` block. This keeps the release commit isolated without polluting your active working directory.
   *   **Step 2: Tag and Push**: Create and push a git tag matching the new version:
       ```bash
       git tag v0.1.3
       git push origin v0.1.3
       ```
       *The pipeline compiles the stable binary using the tag name as the compiled version (e.g., `0.1.3`), and uploads the production `latest.json` manifest directly to the release page.*

       > [!IMPORTANT]
       > **Version Match Guard:** The CI release pipeline validates that the pushed tag version (e.g. `v0.1.3`) matches the static version in `package.json`. If they do not match, the CI job will fail immediately to prevent out-of-sync builds.

For complete instructions on setting up credentials, OIDC, keypairs, and rotation, see the detailed [App Signing Guide](docs/APP_SIGNING.md).

#### Required GitHub Secrets and Variables
To allow the release action to compile, sign, and notarize the packages successfully, configure the following secrets and variables under **Settings > Secrets and variables > Actions** in your GitHub repository:

##### Secrets (Sensitive)
| Secret Name | Description | Platform |
|---|---|---|
| `CODEOBA_TAURI_UPDATE_PRIVATE_KEY_DEV` | Minisign private key content for staging/dev updates. | All (Updater) |
| `CODEOBA_TAURI_UPDATE_PRIVATE_KEY_PASSWORD_DEV` | The password used to encrypt the dev minisign key. | All (Updater) |
| `CODEOBA_TAURI_UPDATE_PRIVATE_KEY_PROD` | Minisign private key content for production updates. | All (Updater) |
| `CODEOBA_TAURI_UPDATE_PRIVATE_KEY_PASSWORD_PROD` | The password used to encrypt the production minisign key. | All (Updater) |
| `MACOS_CERTIFICATE_P12` | Base64-encoded Developer ID Application `.p12` file. | macOS |
| `MACOS_CERTIFICATE_PASSWORD` | Password for the Developer ID Application `.p12` file. | macOS |
| `MACOS_INSTALLER_CERTIFICATE_P12` | Base64-encoded Developer ID Installer `.p12` file. | macOS |
| `MACOS_INSTALLER_CERTIFICATE_PASSWORD` | Password for the Developer ID Installer `.p12` file. | macOS |
| `APPLE_ID_PASSWORD` | App-specific password generated on `appleid.apple.com`. | macOS |
| `AZURE_CLIENT_ID` | Application (client) ID of your Entra ID app registration. | Windows |
| `AZURE_TENANT_ID` | Directory (tenant) ID of your Azure subscription. | Windows |
| `AZURE_SUBSCRIPTION_ID` | ID of your active Azure subscription. | Windows |

##### Variables (Non-Sensitive)
| Variable Name | Description | Platform |
|---|---|---|
| `CODEOBA_TAURI_UPDATE_PUBLIC_KEY_DEV` | Optional staging public key (overrides default in `tauri.conf.json`). | All (Updater) |
| `CODEOBA_TAURI_UPDATE_PUBLIC_KEY_PROD` | Production public key (compiled into production builds). | All (Updater) |
| `APPLE_ID` | Your Apple Developer email address. | macOS |
| `APPLE_TEAM_ID` | Your 10-character Apple Developer Team ID. | macOS |
| `AZURE_SIGNING_ACCOUNT_NAME` | The name of your Azure Artifact Signing Account. | Windows |
| `AZURE_CERTIFICATE_PROFILE_NAME` | The name of your Azure Certificate Profile. | Windows |
| `AZURE_TRUSTED_SIGNING_ENDPOINT` | Azure signing service endpoint (e.g. `https://cus.codesigning.azure.net/`). | Windows |

### 3. CI/CD Linker, Cache, and Notarization Optimizations
To support compilation of native C/C++ dependencies in Rust (such as `esaxx-rs` and `ort`/ONNX Runtime) and run efficient packaging pipelines across different operating systems, the workflow has been optimized with the following configurations:

* **Dynamic MSVC C Runtime Linking (`ESAXX_DYNAMIC_LINK=1`):**  
  Windows MSVC builds use the dynamic CRT (`/MD`) by default. However, some dependencies like `esaxx-rs` historically force static CRT linking (`/MT`), causing `LNK2038` linker mismatches. The workflow compiles a patched dynamic-link-enabled fork and sets `ESAXX_DYNAMIC_LINK: '1'` globally to ensure consistent CRT linkage.
* **Workspace-Relative ONNX Runtime Caching (`ORT_CACHE_DIR`):**  
  By default, `ort` downloads its prebuilt ONNX Runtime binaries into the runner's user home directory (which is not cached). This causes missing binary linker errors on subsequent builds. We redirect downloads to `ort_cache` at the workspace root and cache it separately via `actions/cache` to ensure the binaries are always present alongside Cargo's compiled target objects.
* **Automatic Workflow Cache Invalidation:**  
  The Rust cache step uses `prefix-key: ${{ hashFiles('.github/workflows/build-desktop.yml') }}`. This automatically invalidates the cache whenever the build configuration or environment variables are modified, preventing stale caching issues.
* **Skip Notarization Polling (`SKIP_STAPLING: 'true'`):**  
  By default, macOS notarization wait times can be highly unpredictable, taking hours for complex binaries that contain JIT compilers or embedded ML runtimes (such as `wasmtime` and `ort`). The workflow introduces a global `SKIP_STAPLING` environment variable (default: `'true'`). When enabled, the workflow runs `tauri build -- --skip-stapling`, which submits the bundle to Apple's notarization server but returns immediately. Gatekeeper will verify the notarization online when online macOS clients first run the app. Setting `SKIP_STAPLING` to `'false'` restores full polling and stapling.

