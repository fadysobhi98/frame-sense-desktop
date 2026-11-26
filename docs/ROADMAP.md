# Development Roadmap

## Phase 0 – Environment and architecture
- Verify local Windows prerequisites and versions.
- Verify Tauri/Flutter SDK setup separately.
- Review licenses for candidate libraries.
- Define Desktop/Mobile shared data contracts.

## Phase 1 – Desktop foundation
- Tauri 2 shell.
- Three-zone Dashboard.
- Graphite & Champagne / Obsidian & Copper selectable themes.
- Navigation and status model.
- Build and run on Windows.

## Phase 2 – Projects, database and safe import
- Project creation.
- SQLite metadata.
- Camera 1 / Camera 2 mapping.
- Card/source registration.
- Safe byte copy and SHA-256 verification.
- Preserve source RAW unchanged.
- Collision naming and same-camera duplicate detection.
- One active order constraint and queue model.
- Automated tests with temporary test folders.

## Phase 3 – RAW and color
- Real RAW decoder selected after compatibility/license review.
- Demosaic and high-quality RAW development.
- Camera matching profiles.
- ICC / sRGB default / Adobe RGB option.
- Non-destructive edit settings.
- JPEG 8-bit output and 50–100% quality selection.

## Phase 4 – AI engines
- Face retouch and quality guard.
- Subject/details protection.
- Focus, noise/low-light, mixed lighting/flash.
- Local inference and model version lock.
- Real image dataset testing and quality review.

## Phase 5 – Session tone and classification
- AI session grouping suggestions.
- Multi-image tone preview.
- FS Basic and tone candidates.
- Before/After and fine tuning.
- User approval dependency.
- Multi-camera session tone matching.
- Hall classification.

## Phase 6 – Review, crop, creative and collage
- Quality Review workflow.
- Best Shots (Session-only copies).
- Crop and Instagram Ready.
- AI Creative Set and Collages on demand.
- Smart Output Synchronization.

## Phase 7 – Export and delivery
- JPEG validation.
- Metadata and photographer rights.
- Normal folders and ZIP.
- Smart Re-Editing and optional previous JPEG retention.

## Phase 8 – Mobile
- Flutter SDK/platform setup.
- Android and iOS app foundations.
- Home / Projects / Gallery / Edit / Devices.
- Local project storage and local editing.
- Mobile Export Engine.
- iOS build requires Mac + Xcode.

## Phase 9 – Secure connection and sync
- QR pairing.
- Local Wi-Fi and remote connection.
- Authorized device roles.
- Remote Gallery and encrypted transfer.
- Mobile edit modes and desktop approval safeguards.

## Phase 10 – Release readiness
- Windows/macOS/Android/iOS builds.
- Performance, storage, power and recovery tests.
- Security review.
- Camera compatibility matrix.
- Release packaging and upgrade plan.
