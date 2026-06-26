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
    ./codeoba-tauri --reset-window
    ```

---

## 🧪 Testing

To execute Rust backend tests (including log parsers, search algorithms, and signature checks):
```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

---

## 📦 Building for Production

To compile and package the application into a single platform-specific installer (`.dmg`/`.pkg` on macOS, `.msi` on Windows, `.deb` on Linux):
```bash
npm run tauri build
```
*Tauri automatically compiles the Rust source code in `--release` mode, minifies frontend SolidJS assets, bundles them into the binary, and signs the resulting package if code-signing certificates are configured.*

---

## 🏗️ Project Architecture Map

*   **`src/` (Frontend)**: Styled with Tailwind CSS, SolidJS components manage search layouts, settings panes, markdown renderings, and UI themes.
*   **`src-tauri/` (Backend)**: Rust crate that manages directory watchers (`notify`), SQLite cache databases (`rusqlite`), credential stores (`keyring`), ONNX semantic vector inference (`ort`), and signed WASM plugin loading (`wasmtime`).

---

## 🔄 CI/CD Release Pipeline

### 1. GitHub Actions CI/CD Pipeline
A release pipeline is configured in [.github/workflows/build-desktop.yml](.github/workflows/build-desktop.yml). It automatically triggers, packages, signs (using Apple Developer certificates for macOS and keyless Azure Trusted Signing for Windows via OIDC), and drafts a new GitHub Release when you push a version tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

For complete instructions on setting up Azure OIDC credentials, registering the app client, and exporting Apple developer certificates, see the detailed [App Signing Guide](docs/APP_SIGNING.md).

#### Required GitHub Secrets and Variables
To allow the release action to compile, sign, and notarize the packages successfully, configure the following secrets and variables under **Settings > Secrets and variables > Actions** in your GitHub repository:

##### Secrets (Sensitive)
| Secret Name | Description | Platform |
|---|---|---|
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
| `APPLE_ID` | Your Apple Developer email address. | macOS |
| `APPLE_TEAM_ID` | Your 10-character Apple Developer Team ID. | macOS |
| `AZURE_SIGNING_ACCOUNT_NAME` | The name of your Azure Artifact Signing Account. | Windows |
| `AZURE_CERTIFICATE_PROFILE_NAME` | The name of your Azure Certificate Profile. | Windows |
| `AZURE_TRUSTED_SIGNING_ENDPOINT` | Azure signing service endpoint (e.g. `https://cus.codesigning.azure.net/`). | Windows |

### 2. CI/CD Linker & Cache Optimizations
To support compilation of native C/C++ dependencies in Rust (such as `esaxx-rs` and `ort`/ONNX Runtime) across different operating systems, the workflow has been optimized with the following configurations:

* **Dynamic MSVC C Runtime Linking (`ESAXX_DYNAMIC_LINK=1`):**  
  Windows MSVC builds use the dynamic CRT (`/MD`) by default. However, some dependencies like `esaxx-rs` historically force static CRT linking (`/MT`), causing `LNK2038` linker mismatches. The workflow compiles a patched dynamic-link-enabled fork and sets `ESAXX_DYNAMIC_LINK: '1'` globally to ensure consistent CRT linkage.
* **Workspace-Relative ONNX Runtime Caching (`ORT_CACHE_DIR`):**  
  By default, `ort` downloads its prebuilt ONNX Runtime binaries into the runner's user home directory (which is not cached). This causes missing binary linker errors on subsequent builds. We redirect downloads to `src-tauri/target/ort_cache` so that the binaries are persisted and restored together with Cargo's compiled target objects.
* **Automatic Workflow Cache Invalidation:**  
  The Rust cache step uses `prefix-key: ${{ hashFiles('.github/workflows/build-desktop.yml') }}`. This automatically invalidates the cache whenever the build configuration or environment variables are modified, preventing stale caching issues.

