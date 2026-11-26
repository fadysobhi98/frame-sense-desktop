# Start Frame Sense development

Read these files first:
- docs/FRAME_SENSE_MASTER_SPECIFICATION_AR_V1.1.docx
- docs/FRAME_SENSE_MASTER_SPECIFICATION_AR_V1.1.md
- docs/DECISIONS_AND_OPEN_ITEMS.md
- docs/PROJECT_CONTEXT_FOR_AI.md
- docs/IMPLEMENTATION_STATUS.md

Inspect the actual repository and toolchain before changing anything. Continue from the existing code; do not recreate the project unnecessarily.

First milestone:
1. Verify and build the Tauri Desktop scaffold on Windows.
2. Fix any compile/runtime errors and add reproducible tests for project creation and safe import.
3. Verify the Flutter SDK and Android SDK, then run flutter analyze and an Android debug build.
4. Report exact commands, results, and any blockers.

Do not claim RAW development, AI, color management, production export, remote pairing, authentication, or mobile sync are implemented unless real code and repeatable tests prove it.

Never change approved product decisions. One active order only; Camera 1 and Camera 2 are independent lanes inside that order. Preserve original RAW files. Final output is JPEG 8-bit only, 50–100% selectable, sRGB default and Adobe RGB optional. No TIFF, no 12-bit JPEG, no mandatory cloud AI, no duplicate RAW source copies, and no desktop export overwrite from mobile without explicit approval.
