# Desktop – Tauri 2 + React + Rust

## Current connected foundation
- Tauri desktop shell.
- React/TypeScript three-zone workspace.
- Selectable Graphite and Obsidian theme directions.
- `create_project`: creates the project folders, `project.json` and SQLite metadata database.
- `import_camera_files`: copies supported photo/RAW files as bytes, verifies SHA-256, skips same-camera duplicate hashes and resolves filename collisions without overwriting.

## Not implemented
RAW decoding, AI, color development, JPEG export, processing queue execution, auto card-reader detection, remote transfer, authentication and the rest of the production workflow.

## Run
From this directory:

```powershell
npm install
cargo tauri dev
```

Tauri CLI must be installed first:

```powershell
cargo install tauri-cli --version "^2.0.0" --locked
```
