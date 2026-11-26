# Frame Sense – Test Matrix

## First foundation tests

| ID | Test | Expected result | Current status |
|---|---|---|---|
| FND-001 | Create project with valid base directory | Project root and base folders created | Implemented in starter code; not compiled here |
| FND-002 | Create project with invalid base directory | Clear error; no partial project | Implemented in starter code; not compiled here |
| FND-003 | Import supported image/RAW bytes | Copy into selected camera Originals | Implemented in starter code; not compiled here |
| FND-004 | Source immutability | Source bytes unchanged | Hash verification design; must run on Windows |
| FND-005 | Destination verification | SHA-256 of copy matches source | Implemented in starter code; not compiled here |
| FND-006 | Same-camera duplicate | Skip identical hash in same camera | Implemented in starter code; not compiled here |
| FND-007 | Cross-camera identical file | Keep separate in Camera 1 and Camera 2 | Camera-scoped database constraint |
| FND-008 | Filename collision | Add suffix; never overwrite | Implemented in starter code; not compiled here |
| FND-009 | Unsupported file | Skip and report | Implemented in starter code; not compiled here |
| FND-010 | One active order | Only one Processing state globally | Planned; not wired in starter scaffold |

## Later required tests

- RAW decode quality for representative Sony ARW, Canon CR2/CR3, Nikon NEF, Fujifilm RAF and other tested formats.
- Highlight/shadow recovery and color consistency.
- ICC profile correctness and sRGB/Adobe RGB export.
- JPEG bit depth exactly 8-bit and quality selection 50–100%.
- Face/subject protection and retouch quality.
- Focus/noise/low-light/flash model quality on real photographs.
- Tone approval dependencies: Session waits; other sections continue.
- Multi-camera tone matching without moving/merging source files.
- Best Shots only from Session.
- Church folder grouping.
- Smart Re-Editing overwrite/optional previous JPEG behavior.
- ZIP integrity and final export validation.
- Power setting restoration after processing.
- Cache capacity and safe stop behavior.
- QR pairing, permissions, encryption, remote transfer.
- Mobile sync approval and no desktop export overwrite without approval.
- Crash recovery and checkpoint resume.

## Validation rule

A feature is `Implemented & Tested` only when its code is present, the test is reproducible, and the observed result is recorded. A visible button or mockup is not proof of implementation.
