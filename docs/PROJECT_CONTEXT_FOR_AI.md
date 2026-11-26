# Frame Sense – Agent Instructions

You are working on Frame Sense, a professional local-first AI photography workflow for wedding/event photographers.

## Source of truth
1. `FRAME_SENSE_MASTER_SPECIFICATION_AR_V1.1.docx`
2. `FRAME_SENSE_MASTER_SPECIFICATION_AR_V1.1.md`
3. `DECISIONS_AND_OPEN_ITEMS.md`
4. Original conversation decisions, where available.

Do not silently alter approved product requirements. If a technical conflict appears, explain it and ask before changing product behavior.

## Critical invariants
- Windows + macOS Desktop; Android + iOS Mobile.
- One order only in Processing at a time.
- Camera 1 and Camera 2 are independent lanes inside the same active order.
- RAW original is immutable and is the source for every re-edit.
- No duplicate RAW originals or RAW sorted copy folders.
- Final exports are JPEG 8-bit only; no TIFF and no 12-bit JPEG.
- JPEG quality 50–100% selectable; original dimensions default.
- sRGB default, Adobe RGB optional, embed ICC.
- AI core local/free; no mandatory cloud AI.
- Session tone waits for explicit approval; other sections continue on Smart Basic.
- Best Shots only from Session, as copies; originals remain in Session.
- Church + Katb Ketab together; Makeup separate from Getting Ready.
- Hall categories are selected by AI based on actual content; no empty categories.
- Mobile edits never overwrite Desktop Export without explicit approval.
- Optional FS Account syncs profile/preferences only, not photos or RAW.
- Both approved themes are selectable; neither is a mandated default.

## Engineering rules
- First inspect the actual repository and toolchain.
- Do not claim code is built or tested unless it was run.
- Mark each requirement Implemented & Tested / Partial / Planned / Blocked.
- Preserve original files and test import with hashes.
- Avoid unnecessary copies, snapshots and cache growth.
- Review dependency licenses before selecting RAW/AI libraries.
- Never invent AI model capability or camera compatibility.
- Keep Windows and macOS Desktop in scope from the start.
- Flutter is a starter candidate; validate before treating it as a final architecture decision.
- iOS/macOS build requires Mac + Xcode.
- No placeholder screen counts as a delivered feature.

## First tasks
1. Inspect this repository and confirm tool versions.
2. Make the Desktop scaffold build on Windows.
3. Make the Flutter scaffold pass `flutter analyze` and run on Android once SDK is configured.
4. Add automated tests for project creation and import safety.
5. Only then extend the processing engine.
