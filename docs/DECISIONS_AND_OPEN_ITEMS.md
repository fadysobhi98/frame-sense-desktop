# Frame Sense – Decisions and Open Items

## Approved product decisions

- Desktop scope: Windows and macOS.
- Mobile scope: Android and iOS together.
- One order only may be in `Processing` at a time. Other orders wait in the queue.
- Camera 1 and Camera 2 are independent processing lanes **inside the same active order**.
- Connecting a card reader triggers detection/preflight in the intended product flow; it does not start processing. The user presses Start.
- Preserve original filenames as metadata; resolve filename collisions without overwriting.
- Keep one original RAW source file per image. Never modify the RAW original or create derivative RAW source copies.
- Read RAW directly; do not use an intermediate JPEG as the source for RAW development.
- Internal processing: high precision (16-bit and/or floating point as appropriate), wide-gamut working space.
- Final outputs: JPEG 8-bit only. No TIFF export and no 12-bit JPEG.
- JPEG quality selectable from 50% to 100% per order/profile. Original dimensions are default unless crop/resize is requested.
- sRGB default; Adobe RGB optional; embed ICC profile.
- FS Basic is the baseline for tone preview. Multi-Image Preview and Fine-Tuning are approved.
- AI suggests session groups; user accepts/rejects.
- Session Tone requires user approval. If not approved, Session waits while other sections continue with Smart Basic.
- Multi-Camera Color Matching keeps a consistent Session Tone across cameras while camera files/folders remain separate.
- Best Shots are separate output copies from Session only; originals stay in Session.
- Church and Katb Ketab remain grouped.
- Makeup remains separate from Getting Ready.
- Hall classification is content-based and creates only relevant categories.
- Smart Re-Editing replaces the previous JPEG by default only after successful validation; optional previous JPEG retention is not a full edit-history system.
- Smart Output Synchronization updates Export, Crop, Instagram Ready and Best Shots; AI Creative and Collages update on demand.
- Delivery supports normal folders and ZIP, with user-selected sections.
- Mobile Export saves inside app projects; optional extra Gallery save is user-controlled.
- Mobile edits never overwrite desktop Export without explicit approval.
- FS Account is optional: Google, Email, or Guest. Sync selected profile/preferences only; photos and RAW remain local.
- Secure pairing: QR, local Wi-Fi and remote internet, encrypted transfer/connection, authorized-device permissions.
- Themes: Graphite & Champagne and Obsidian & Copper, both selectable. No default theme is approved.
- Statuses: Completed, Review Required, Error. Notifications: desktop and authorized mobile devices.
- Smart Balanced default performance; Live Performance Switching; Smart Power only during processing then restore prior power settings; Smart Storage chooses cache location with manual override; AI Model Version Lock.
- AI core must remain local/free; no mandatory cloud AI or per-image AI fee.

## Approved export folder structure

```text
Project Root — Client Name - Event Type - Date
├── Originals
│   ├── Camera 1
│   └── Camera 2
├── Export
│   ├── Camera 1
│   │   ├── Makeup
│   │   ├── Getting Ready
│   │   ├── Session
│   │   ├── Best Shots
│   │   ├── Church
│   │   ├── Hall & Party (AI-selected categories)
│   │   ├── Crop
│   │   ├── Instagram Ready
│   │   ├── AI Creative Set
│   │   ├── Collages
│   │   └── Quality Review
│   └── Camera 2
│       └── Same independent relevant sections
├── Mobile Edits
└── Project Data
```

Only create category folders when relevant; do not create unnecessary empty folders.

## Technical candidates – not finally approved

- Tauri 2 + React/TypeScript for Desktop: candidate.
- Rust core: candidate.
- Flutter for Mobile: candidate/working scaffold, subject to platform and performance validation.
- SQLite for project metadata: required foundation.
- LibRaw or compatible alternative: candidate; validate camera coverage, output quality and license.
- ONNX Runtime or alternative: candidate; validate model compatibility, CPU/GPU performance and license.
- LittleCMS or equivalent: candidate; validate ICC workflow.
- Remote signaling/relay: still open; secure and low-cost approach must be evaluated.

## Explicitly excluded / do not add without approval

- Mandatory cloud AI.
- Per-image AI fee for core AI.
- TIFF or 12-bit JPEG output.
- Duplicate RAW originals or RAW `Sorted` folder.
- Automatic deletion/deduplication across Camera 1 and Camera 2.
- Full edit-history/version-control/snapshot system.
- Automatic overwrite of desktop exports by mobile edits.
- AI Wedding Storytelling.
- Logo Studio, watermark/logo addition, and Background Cleanup were seen in older concepts but are not approved product features.
- Tone names in old mockups (Natural, Champagne, Cinematic, Warm Film, Moody, Teal) are examples, not an approved final preset library.

## Open items

- Exact camera/RAW compatibility matrix after real sample tests.
- RAW decoder choice and license.
- AI runtime/model catalog and model licenses.
- Final tone preset names beyond FS Basic.
- Remote relay/signaling architecture and cost.
- Exact UI control for JPEG quality (slider or presets).
- macOS and iOS build/test environment using Mac + Xcode.
- Re-run and verify earlier prototype tests; the V1.1 source report mentions 6 passed tests but lists 7 cases.
