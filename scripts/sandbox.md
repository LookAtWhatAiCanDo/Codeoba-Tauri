# Interactive Testing Sandbox Guide

This sandbox allows you to visually verify Codeoba's real-time features—including the filesystem watcher, SQLite database change reloader, and reactive frontend deletion flows—without touching your actual production log directories.

---

## 1. Quick Start

### Step 1: Initialize the Empty Sandbox
Run the generator script with the `--init` flag to clear the mock environment and set up empty target subdirectories:
```bash
python3 scripts/sandbox.py --init
```

### Step 2: Launch the App in Sandbox Mode
Start the Tauri application with the `CODEOBA_MOCK_HOME` environment variable pointing to the `demo_mock` directory:
```bash
CODEOBA_MOCK_HOME=$PWD/demo_mock npm run tauri dev
```
*The application will boot successfully with an empty left sidebar (0 sessions).*

---

## 2. Interactive CLI Controls

While the app is running in the background, open a separate terminal window and use the following arguments to inject, modify, or delete sessions.

### Session Injection
Generate mock log profiles individually or all at once:
* **All 6 Providers:** `python3 scripts/sandbox.py --all`
* **Aider (Markdown):** `python3 scripts/sandbox.py --aider`
* **Antigravity (Protobuf):** `python3 scripts/sandbox.py --antigravity`
* **Claude Code (JSONL):** `python3 scripts/sandbox.py --claude`
* **Cursor (SQLite):** `python3 scripts/sandbox.py --cursor`
* **GitHub Copilot (YAML):** `python3 scripts/sandbox.py --copilot`
* **OpenAI Codex (JSONL):** `python3 scripts/sandbox.py --codex`

*Watch the app: Within 500ms of running any of these, the session card will slide into the left sidebar list.*

### Modify Events (Appending Turns)
Append extra turns to an active mock session log to test the modify watcher and reactive UI updates:
```bash
python3 scripts/sandbox.py --add-turn [cursor|claude|aider|copilot|codex|antigravity]
```
*Watch the app: The session card's timestamp updates, and clicking on it reveals the newly appended turns instantly.*

### Deletion Events
Delete the mock logs for a specific source to test the deletion watcher and UI cleanup:
```bash
python3 scripts/sandbox.py --delete [cursor|claude|aider|copilot|codex|antigravity]
```
*Watch the app: The session is immediately removed from the search index, the card slides out of the sidebar, and the active view resets if that session was open.*

---

## 3. Directory Layout inside `demo_mock/`

When a session is injected, the script writes files to paths matching the standard directory layout for the target OS:
* **Cursor:** `Library/Application Support/Cursor/User/` (or `AppData/Roaming` on Windows) containing global and workspace `state.vscdb` databases.
* **Claude Code:** `.claude/projects/` containing nested project log `.jsonl` files and `.claude/plans/` markdown files.
* **Aider:** `Dev/` containing `.aider.chat.history.md` markdown files.
* **GitHub Copilot:** `.copilot/session-state/` containing `workspace.yaml` and `events.jsonl` files.
* **OpenAI Codex:** `.codex/` containing `session_index.jsonl` and rollout JSONL logs.
* **Antigravity:** `.gemini/antigravity/` containing summaries protocol buffers and transcript JSONL logs.
