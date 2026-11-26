# Implementation Status – Starter Kit

Date of starter-kit generation: 2026-11-17.

## Local Windows environment reported in the conversation

- Rust and Cargo: version output reported as 1.100.0.
- MSVC x64 compiler: activated and `cl` showed x64 target.
- Node.js: version output reported as v24.22.0.
- npm: version output reported as 11.19.1.
- Git: installed and version output reported as 2.56.1.
- Android Studio: user confirmed installation.
- Tauri CLI: not confirmed installed.
- Flutter SDK: not confirmed installed; installing Android Studio or Flutter/Dart editor plugins alone does not install Flutter SDK.
- Android SDK components: not yet verified.
- Mac + Xcode: not available/verified in the Windows setup conversation.

These are conversation-reported statuses, not a fresh scan of the user's computer.

## Starter kit status

- Desktop source scaffold: created.
- Desktop UI shell: created as prototype.
- Project folder creation command: included, not compiled or run on user's PC yet.
- Safe copy/import command: included, not compiled or run on user's PC yet.
- SQLite project schema: included.
- Flutter source scaffold: created, not run because Flutter SDK is not verified.
- UI reference images: included.
- RAW development: planned, not implemented.
- AI models: planned, not implemented.
- Production color pipeline/export: planned, not implemented.
- Remote pairing/encrypted transfer: planned, not implemented.
- Authentication/profile sync: planned, not implemented.
- macOS/iOS build: requires Mac + Xcode and is not tested.

## Important environment distinction

Tools installed on the user's Windows PC do not automatically become available inside ChatGPT Work or another cloud development environment. The code package can be opened locally in VS Code; a cloud agent needs its own repository/workspace and toolchain.

## Update 2026-11-26 (Instinct work session)

Verified by actually running code:

- Frontend `npm run build` (tsc --noEmit + vite build): passes, no type errors.
- Core logic extracted to `desktop/src-tauri/src/core.rs` (no Tauri dependency) with 12 automated tests, all passing. They cover test matrix FND-001 through FND-009 plus nested-folder guard, unknown camera guard and non-project folder guard. Run them with `cargo test` in `desktop/src-tauri`.
- App icons generated from `desktop/frame-sense-logo.png.png` into `desktop/src-tauri/icons/` (were missing from the starter kit; the Windows build requires them).
- GitHub Actions workflow `.github/workflows/build-windows.yml` added: on every push to `main` it runs the frontend install, the core tests and a full Tauri build on `windows-latest`, then attaches the NSIS installer (.exe) to a GitHub Release.

Not yet verified:

- Full Tauri compile (`cargo build` of the whole app) and the NSIS installer itself: first run happens in GitHub Actions on Windows.
- App runtime behavior on Fady's Windows PC.
- Everything from Phase 3 onward (RAW, AI, color, export, mobile, pairing) remains planned, not implemented.

