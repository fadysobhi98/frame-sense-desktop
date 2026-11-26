# Import safety notes

The current command is a starter byte-copy importer:
- It accepts a curated list of photo and RAW file extensions.
- It does not decode or edit the source.
- It computes SHA-256 for the source and copied destination.
- It skips a matching hash already imported to the same camera.
- It does not deduplicate across Camera 1 and Camera 2.
- It resolves filename collisions with a suffix instead of overwriting.
- It records the original filename and source path in SQLite.

Limitations:
- No card-reader hardware watcher or automatic mount detection yet.
- No card eject/lock handling.
- No recovery journal for a crash during a multi-file import yet.
- Unsupported files are skipped and counted; the detailed skipped filename list is not yet exposed.
- This code has not been compiled or run on the user's Windows machine as part of this package generation.
