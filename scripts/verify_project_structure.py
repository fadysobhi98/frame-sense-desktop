from pathlib import Path
import json
root = Path(__file__).resolve().parents[1]
required = [
    "README_AR.md",
    "docs/FRAME_SENSE_MASTER_SPECIFICATION_AR_V1.1.docx",
    "desktop/package.json",
    "desktop/src-tauri/Cargo.toml",
    "desktop/src/App.tsx",
    "mobile/pubspec.yaml",
    "mobile/lib/main.dart",
    "shared/folder-structure.json",
]
missing = [p for p in required if not (root / p).exists()]
if missing:
    raise SystemExit("Missing files: " + ", ".join(missing))
json.loads((root / "shared/folder-structure.json").read_text(encoding="utf-8"))
json.loads((root / "shared/project.schema.json").read_text(encoding="utf-8"))
print("Frame Sense starter-kit structure: OK")
