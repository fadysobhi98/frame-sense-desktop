//! Frame Sense core: project creation and safe import.
//!
//! This module has no Tauri dependencies so the logic can be tested
//! with `cargo test` on any platform. The Tauri commands in `lib.rs`
//! are thin wrappers over these functions.

use rusqlite::{params, Connection};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Serialize)]
pub struct CreateProjectResult {
    pub project_id: String,
    pub project_name: String,
    pub project_path: String,
}

#[derive(Serialize)]
pub struct ImportResult {
    pub imported: usize,
    pub duplicates: usize,
    pub skipped_unsupported: usize,
    pub camera_id: String,
    pub destination: String,
}

fn now_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn safe_segment(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.');
    if cleaned.is_empty() {
        "Untitled".to_string()
    } else {
        cleaned.to_string()
    }
}

fn create_project_schema(
    db_path: &Path,
    project_id: &str,
    project_name: &str,
) -> Result<(), String> {
    let conn =
        Connection::open(db_path).map_err(|e| format!("Could not open project database: {e}"))?;
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS project_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS camera_sources (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            camera_id TEXT NOT NULL CHECK(camera_id IN ('Camera 1', 'Camera 2')),
            source_path TEXT NOT NULL,
            added_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS imported_files (
            id TEXT PRIMARY KEY,
            camera_id TEXT NOT NULL CHECK(camera_id IN ('Camera 1', 'Camera 2')),
            source_path TEXT NOT NULL,
            original_name TEXT NOT NULL,
            stored_name TEXT NOT NULL,
            relative_path TEXT NOT NULL,
            absolute_path TEXT NOT NULL DEFAULT '',
            sha256 TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            imported_at TEXT NOT NULL,
            UNIQUE(camera_id, sha256)
        );
        ",
    )
    .map_err(|e| format!("Could not initialize project database: {e}"))?;
    conn.execute(
        "INSERT OR REPLACE INTO project_meta(key, value) VALUES('project_id', ?1)",
        params![project_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO project_meta(key, value) VALUES('project_name', ?1)",
        params![project_name],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO project_meta(key, value) VALUES('schema_version', '1')",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Open a project database and apply any pending schema migrations.
fn open_project_db(project_root: &Path) -> Result<Connection, String> {
    let db_path = project_root.join("Project Data").join("project.sqlite");
    if !db_path.is_file() {
        return Err("Project database is missing.".into());
    }
    let conn =
        Connection::open(&db_path).map_err(|e| format!("Could not open project database: {e}"))?;

    // Migration (pre-0.2 projects): absolute_path on imported_files.
    if conn
        .prepare("SELECT absolute_path FROM imported_files LIMIT 0")
        .is_err()
    {
        conn.execute_batch(
            "ALTER TABLE imported_files ADD COLUMN absolute_path TEXT NOT NULL DEFAULT ''",
        )
        .map_err(|e| format!("Could not migrate project database: {e}"))?;
        let mut stmt = conn
            .prepare("SELECT id, relative_path FROM imported_files")
            .map_err(|e| e.to_string())?;
        let rows: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();
        for (id, rel) in rows {
            let abs = project_root.join(&rel).to_string_lossy().to_string();
            conn.execute(
                "UPDATE imported_files SET absolute_path = ?1 WHERE id = ?2",
                params![abs, id],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS auto_corrections (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_sha256 TEXT NOT NULL,
            camera_id TEXT NOT NULL,
            params TEXT NOT NULL,
            source_path TEXT NOT NULL,
            output_path TEXT NOT NULL,
            created_at TEXT NOT NULL,
            UNIQUE(source_sha256, params)
        );
        ",
    )
    .map_err(|e| format!("Could not initialize auto-correction log: {e}"))?;
    Ok(conn)
}

pub fn create_project(
    base_directory: &str,
    client_name: &str,
    event_type: &str,
    event_date: &str,
) -> Result<CreateProjectResult, String> {
    let base = PathBuf::from(base_directory.trim());
    if !base.is_dir() {
        return Err(
            "The parent folder must already exist. Choose an existing folder on a local drive."
                .into(),
        );
    }
    let client = safe_segment(client_name);
    let event = safe_segment(event_type);
    let date = safe_segment(event_date);
    let project_name = format!("{client} - {event} - {date}");
    let mut project_root = base.join(&project_name);
    let mut suffix = 2;
    while project_root.exists() {
        project_root = base.join(format!("{project_name} ({suffix})"));
        suffix += 1;
    }

    let project_id = Uuid::new_v4().to_string();
    let directories = [
        "Originals/Camera 1",
        "Originals/Camera 2",
        "Export/Camera 1",
        "Export/Camera 2",
        "Mobile Edits",
        "Project Data",
    ];
    for relative in directories {
        fs::create_dir_all(project_root.join(relative))
            .map_err(|e| format!("Could not create {relative}: {e}"))?;
    }

    let db_path = project_root.join("Project Data").join("project.sqlite");
    create_project_schema(&db_path, &project_id, &project_name)?;

    let manifest = serde_json::json!({
        "schemaVersion": 1,
        "projectId": project_id,
        "clientName": client,
        "eventType": event,
        "eventDate": date,
        "createdAt": now_string(),
        "cameras": ["Camera 1", "Camera 2"],
        "output": {
            "format": "JPEG",
            "bitDepth": 8,
            "qualityMin": 50,
            "qualityMax": 100,
            "defaultColorSpace": "sRGB",
            "optionalColorSpace": "Adobe RGB",
            "preserveOriginalDimensionsByDefault": true
        },
        "processing": {
            "maxActiveOrders": 1,
            "cameraLanesIndependentWithinActiveOrder": true
        }
    });
    let manifest_path = project_root.join("Project Data").join("project.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Could not write project manifest: {e}"))?;

    Ok(CreateProjectResult {
        project_id,
        project_name,
        project_path: project_root.to_string_lossy().to_string(),
    })
}

/// Photo extensions the app accepts (RAW + everyday raster).
pub fn is_supported_photo(path: &Path) -> bool {
    let allowed = [
        // Everyday image formats
        "jpg", "jpeg", "png", "webp", "heic", "heif",
        // Sony
        "arw", "srf", "sr2",
        // Canon
        "cr2", "cr3", "crw",
        // Nikon
        "nef", "nrw",
        // Fujifilm
        "raf",
        // Olympus / OM System
        "orf",
        // Panasonic / Leica
        "rw2", "rwl",
        // Pentax / Ricoh
        "pef", "ptx",
        // Samsung
        "srw",
        // Adobe / generic DNG
        "dng",
        // Hasselblad
        "3fr", "fff",
        // Phase One
        "iiq",
        // Sigma
        "x3f",
        // Minolta / Konica Minolta
        "mrw",
        // Epson
        "erf",
        // Mamiya / Leaf
        "mef", "mos",
        // Kodak
        "dcr", "k25", "kdc",
        // GoPro
        "gpr",
        // Casio
        "bay",
        // Generic RAW extension used by some cameras
        "raw",
    ];
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| allowed.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn unique_destination(directory: &Path, original_name: &str) -> PathBuf {
    let original = Path::new(original_name);
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");
    let extension = original.extension().and_then(|s| s.to_str()).unwrap_or("");
    let first = directory.join(original_name);
    if !first.exists() {
        return first;
    }
    for index in 1..100_000 {
        let candidate_name = if extension.is_empty() {
            format!("{stem}_{index:03}")
        } else {
            format!("{stem}_{index:03}.{extension}")
        };
        let candidate = directory.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{}_{}", stem, Uuid::new_v4()))
}

pub fn import_camera_files(
    project_path: &str,
    source_path: &str,
    camera_id: &str,
    destination_dir: Option<&str>,
) -> Result<ImportResult, String> {
    if camera_id != "Camera 1" && camera_id != "Camera 2" {
        return Err("Camera must be Camera 1 or Camera 2.".into());
    }
    let project_root = PathBuf::from(project_path.trim())
        .canonicalize()
        .map_err(|e| format!("Project folder not found: {e}"))?;
    let source_root = PathBuf::from(source_path.trim())
        .canonicalize()
        .map_err(|e| format!("Card/source folder not found: {e}"))?;
    if !source_root.is_dir() {
        return Err("The card/source path must be a folder.".into());
    }
    // Prevent recursive self-import when the destination is inside the scanned source tree.
    if project_root.starts_with(&source_root) || source_root.starts_with(&project_root) {
        return Err(
            "The project folder and card/source folder must be separate, non-nested folders."
                .into(),
        );
    }

    // Destination: the project's Originals/<camera> folder by default, or a
    // folder the user picked themselves (keeping the Originals/<camera>
    // structure underneath it).
    let destination_dir = match destination_dir.map(str::trim).filter(|d| !d.is_empty()) {
        None => project_root.join("Originals").join(camera_id),
        Some(picked) => {
            let picked_root = PathBuf::from(picked)
                .canonicalize()
                .map_err(|e| format!("Destination folder not found: {e}"))?;
            if !picked_root.is_dir() {
                return Err("The destination must be a folder.".into());
            }
            if picked_root.starts_with(&source_root) || source_root.starts_with(&picked_root) {
                return Err(
                    "The destination folder and card/source folder must be separate, non-nested folders."
                        .into(),
                );
            }
            let dir = picked_root.join("Originals").join(camera_id);
            fs::create_dir_all(&dir)
                .map_err(|e| format!("Could not create the destination camera folder: {e}"))?;
            dir
        }
    };
    if !destination_dir.is_dir() {
        return Err(
            "This does not look like a Frame Sense project. Originals/Camera folder is missing."
                .into(),
        );
    }
    let conn = open_project_db(&project_root)?;
    conn.execute(
        "INSERT INTO camera_sources(camera_id, source_path, added_at) VALUES(?1, ?2, ?3)",
        params![
            camera_id,
            source_root.to_string_lossy().to_string(),
            now_string()
        ],
    )
    .map_err(|e| e.to_string())?;

    let mut imported = 0usize;
    let mut duplicates = 0usize;
    let mut skipped_unsupported = 0usize;

    for entry in WalkDir::new(&source_root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let source_file = entry.path();
        if !is_supported_photo(source_file) {
            skipped_unsupported += 1;
            continue;
        }

        let source_hash = sha256_file(source_file)?;
        let already_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM imported_files WHERE camera_id = ?1 AND sha256 = ?2)",
                params![camera_id, source_hash],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if already_exists {
            duplicates += 1;
            continue;
        }

        let original_name = source_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image")
            .to_string();
        let destination_file = unique_destination(&destination_dir, &original_name);
        fs::copy(source_file, &destination_file)
            .map_err(|e| format!("Copy failed for {}: {e}", original_name))?;

        let copied_hash = sha256_file(&destination_file)?;
        if copied_hash != source_hash {
            let _ = fs::remove_file(&destination_file);
            return Err(format!(
                "SHA-256 verification failed for {original_name}. The copy was removed."
            ));
        }

        let relative = destination_file
            .strip_prefix(&project_root)
            .unwrap_or(&destination_file)
            .to_string_lossy()
            .replace('\\', "/");
        let size = fs::metadata(&destination_file)
            .map_err(|e| e.to_string())?
            .len() as i64;

        conn.execute(
            "INSERT INTO imported_files(id, camera_id, source_path, original_name, stored_name, relative_path, absolute_path, sha256, size_bytes, imported_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                Uuid::new_v4().to_string(),
                camera_id,
                source_file.to_string_lossy().to_string(),
                original_name,
                destination_file.file_name().and_then(|n| n.to_str()).unwrap_or("image"),
                relative,
                destination_file.to_string_lossy().to_string(),
                source_hash,
                size,
                now_string()
            ],
        ).map_err(|e| e.to_string())?;
        imported += 1;
    }

    Ok(ImportResult {
        imported,
        duplicates,
        skipped_unsupported,
        camera_id: camera_id.to_string(),
        destination: destination_dir.to_string_lossy().to_string(),
    })
}

// ---------------------------------------------------------------------------
// Phase 2: Smart Basic auto-correction
// ---------------------------------------------------------------------------

pub const DEFAULT_JPEG_QUALITY: u8 = 90;
pub const SMART_BASIC_PARAMS_VERSION: &str = "smart-basic-v1";

#[derive(Serialize, Clone)]
pub struct AutoCorrectProgress {
    pub current: usize,
    pub total: usize,
    pub file: String,
    pub processed: usize,
    pub failed: usize,
}

#[derive(Serialize, Debug)]
pub struct AutoCorrectFailure {
    pub file: String,
    pub error: String,
}

#[derive(Serialize)]
pub struct AutoCorrectReport {
    pub total: usize,
    pub processed: usize,
    pub already_done: usize,
    pub failed: Vec<AutoCorrectFailure>,
    pub output_dir: String,
}

/// Run Smart Basic auto-correction over one camera lane of a project.
///
/// `correct` performs one file (decode + correct + JPEG encode). In the app
/// this calls framesense_raw.dll; tests inject the engine directly.
/// Originals are never modified: outputs go to a separate folder as new
/// 8-bit JPEGs, and finished work is recorded so re-runs skip it.
pub fn run_auto_correct(
    project_path: &str,
    camera_id: &str,
    output_dir: Option<&str>,
    quality: u8,
    correct: &dyn Fn(&Path, &Path, u8) -> Result<(), String>,
    progress: &dyn Fn(AutoCorrectProgress),
) -> Result<AutoCorrectReport, String> {
    if camera_id != "Camera 1" && camera_id != "Camera 2" {
        return Err("Camera must be Camera 1 or Camera 2.".into());
    }
    let quality = quality.clamp(50, 100);
    let project_root = PathBuf::from(project_path.trim())
        .canonicalize()
        .map_err(|e| format!("Project folder not found: {e}"))?;
    let conn = open_project_db(&project_root)?;

    let output_root = match output_dir.map(str::trim).filter(|d| !d.is_empty()) {
        Some(picked) => {
            let root = PathBuf::from(picked);
            fs::create_dir_all(&root)
                .map_err(|e| format!("Could not create the output folder: {e}"))?;
            root.canonicalize()
                .map_err(|e| format!("Output folder not usable: {e}"))?
        }
        None => project_root.join("Export"),
    };
    if output_root.starts_with(project_root.join("Originals")) {
        return Err("The output folder must not be inside Originals.".into());
    }
    let output_camera_dir = output_root.join(camera_id);
    fs::create_dir_all(&output_camera_dir)
        .map_err(|e| format!("Could not create the output camera folder: {e}"))?;

    let params_key = format!("{SMART_BASIC_PARAMS_VERSION}:q{quality}");

    let mut stmt = conn
        .prepare(
            "SELECT id, original_name, relative_path, absolute_path, sha256
             FROM imported_files WHERE camera_id = ?1 ORDER BY imported_at, rowid",
        )
        .map_err(|e| e.to_string())?;
    let files: Vec<(String, String, String, String, String)> = stmt
        .query_map(params![camera_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();

    let total = files.len();
    let mut processed = 0usize;
    let mut already_done = 0usize;
    let mut failed: Vec<AutoCorrectFailure> = Vec::new();

    for (index, (_id, original_name, relative_path, absolute_path, sha256)) in
        files.into_iter().enumerate()
    {
        progress(AutoCorrectProgress {
            current: index + 1,
            total,
            file: original_name.clone(),
            processed,
            failed: failed.len(),
        });

        let source_file = {
            let abs = PathBuf::from(&absolute_path);
            if !absolute_path.is_empty() && abs.is_file() {
                abs
            } else {
                project_root.join(&relative_path)
            }
        };
        if !source_file.is_file() {
            failed.push(AutoCorrectFailure {
                file: original_name.clone(),
                error: "original file not found on disk".to_string(),
            });
            continue;
        }

        let done: Option<String> = conn
            .query_row(
                "SELECT output_path FROM auto_corrections WHERE source_sha256 = ?1 AND params = ?2",
                params![sha256, params_key],
                |row| row.get(0),
            )
            .ok();
        if let Some(existing) = done {
            if Path::new(&existing).is_file() {
                already_done += 1;
                continue;
            }
        }

        let stem = Path::new(&original_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image");
        let output_file = unique_destination(&output_camera_dir, &format!("{stem}.jpg"));

        match correct(&source_file, &output_file, quality) {
            Ok(()) => {
                conn.execute(
                    "INSERT OR REPLACE INTO auto_corrections(source_sha256, camera_id, params, source_path, output_path, created_at)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        sha256,
                        camera_id,
                        params_key,
                        source_file.to_string_lossy().to_string(),
                        output_file.to_string_lossy().to_string(),
                        now_string()
                    ],
                )
                .map_err(|e| e.to_string())?;
                processed += 1;
            }
            Err(e) => {
                let _ = fs::remove_file(&output_file);
                failed.push(AutoCorrectFailure {
                    file: original_name.clone(),
                    error: e,
                });
            }
        }
    }

    progress(AutoCorrectProgress {
        current: total,
        total,
        file: String::new(),
        processed,
        failed: failed.len(),
    });

    Ok(AutoCorrectReport {
        total,
        processed,
        already_done,
        failed,
        output_dir: output_camera_dir.to_string_lossy().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "framesense-test-{label}-{}",
                Uuid::new_v4()
            ));
            fs::create_dir_all(&dir).expect("create temp test dir");
            TestDir(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, bytes).unwrap();
        path
    }

    fn make_project(base: &TestDir) -> CreateProjectResult {
        create_project(
            base.path().to_str().unwrap(),
            "Test Client",
            "Wedding",
            "2026-11-26",
        )
        .expect("project creation should succeed")
    }

    /// FND-001: valid base directory creates project root and base folders.
    #[test]
    fn fnd_001_create_project_valid() {
        let base = TestDir::new("fnd001");
        let result = make_project(&base);
        let root = PathBuf::from(&result.project_path);
        assert_eq!(result.project_name, "Test Client - Wedding - 2026-11-26");
        for rel in [
            "Originals/Camera 1",
            "Originals/Camera 2",
            "Export/Camera 1",
            "Export/Camera 2",
            "Mobile Edits",
            "Project Data/project.sqlite",
            "Project Data/project.json",
        ] {
            assert!(root.join(rel).exists(), "missing {rel}");
        }
    }

    /// FND-002: invalid base directory errors and creates nothing.
    #[test]
    fn fnd_002_create_project_invalid_base() {
        let base = TestDir::new("fnd002");
        let missing = base.path().join("does-not-exist");
        let result = create_project(missing.to_str().unwrap(), "A", "B", "2026-01-01");
        assert!(result.is_err());
        assert!(!missing.exists());
    }

    /// Project names are sanitized for the filesystem.
    #[test]
    fn create_project_sanitizes_names() {
        let base = TestDir::new("sanitize");
        let result = create_project(
            base.path().to_str().unwrap(),
            "A/B:C*D?",
            "Wed/ding",
            "2026.11.26.",
        )
        .unwrap();
        assert_eq!(result.project_name, "A_B_C_D_ - Wed_ding - 2026.11.26");
    }

    /// A second project with the same identity gets a suffix, never an overwrite.
    #[test]
    fn create_project_collision_suffix() {
        let base = TestDir::new("collision");
        let first = make_project(&base);
        let second = make_project(&base);
        assert_ne!(first.project_path, second.project_path);
        assert!(second.project_path.ends_with("(2)"));
        assert!(PathBuf::from(&first.project_path).exists());
        assert!(PathBuf::from(&second.project_path).exists());
    }

    /// FND-003/004/005: import copies supported files, source bytes unchanged,
    /// destination SHA-256 recorded and matching.
    #[test]
    fn fnd_003_004_005_import_copies_and_verifies() {
        let base = TestDir::new("fnd003-base");
        let card = TestDir::new("fnd003-card");
        let project = make_project(&base);

        let raw_bytes: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let source = write_file(card.path(), "DCIM/100SONY/DSC0001.ARW", &raw_bytes);
        let hash_before = sha256_file(&source).unwrap();

        let result = import_camera_files(
            &project.project_path,
            card.path().to_str().unwrap(),
            "Camera 1",
            None,
        )
        .unwrap();

        assert_eq!(result.imported, 1);
        assert_eq!(result.duplicates, 0);
        // FND-004: source unchanged.
        assert_eq!(hash_before, sha256_file(&source).unwrap());
        // FND-005: stored copy matches.
        let copied = PathBuf::from(&project.project_path)
            .join("Originals/Camera 1/DSC0001.ARW");
        assert!(copied.exists());
        assert_eq!(hash_before, sha256_file(&copied).unwrap());
        // Database recorded the hash.
        let conn = Connection::open(
            PathBuf::from(&project.project_path).join("Project Data/project.sqlite"),
        )
        .unwrap();
        let stored_hash: String = conn
            .query_row(
                "SELECT sha256 FROM imported_files WHERE stored_name = 'DSC0001.ARW'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_hash, hash_before);
    }

    /// FND-006: importing the same bytes into the same camera is a duplicate skip.
    #[test]
    fn fnd_006_same_camera_duplicate_skipped() {
        let base = TestDir::new("fnd006-base");
        let card = TestDir::new("fnd006-card");
        let project = make_project(&base);
        write_file(card.path(), "A.jpg", b"same bytes");

        let first = import_camera_files(&project.project_path, card.path().to_str().unwrap(), "Camera 1", None).unwrap();
        let second = import_camera_files(&project.project_path, card.path().to_str().unwrap(), "Camera 1", None).unwrap();
        assert_eq!(first.imported, 1);
        assert_eq!(second.imported, 0);
        assert_eq!(second.duplicates, 1);
    }

    /// FND-007: identical bytes stay separate across Camera 1 and Camera 2.
    #[test]
    fn fnd_007_cross_camera_identical_kept() {
        let base = TestDir::new("fnd007-base");
        let card = TestDir::new("fnd007-card");
        let project = make_project(&base);
        write_file(card.path(), "A.jpg", b"same bytes");

        let cam1 = import_camera_files(&project.project_path, card.path().to_str().unwrap(), "Camera 1", None).unwrap();
        let cam2 = import_camera_files(&project.project_path, card.path().to_str().unwrap(), "Camera 2", None).unwrap();
        assert_eq!(cam1.imported, 1);
        assert_eq!(cam2.imported, 1);
        assert!(PathBuf::from(&project.project_path).join("Originals/Camera 1/A.jpg").exists());
        assert!(PathBuf::from(&project.project_path).join("Originals/Camera 2/A.jpg").exists());
    }

    /// FND-008: filename collisions get a suffix; nothing is overwritten.
    #[test]
    fn fnd_008_filename_collision_suffix() {
        let base = TestDir::new("fnd008-base");
        let card = TestDir::new("fnd008-card");
        let project = make_project(&base);
        write_file(card.path(), "slot1/IMG_0001.jpg", b"first photo bytes");
        write_file(card.path(), "slot2/IMG_0001.jpg", b"different photo bytes!");

        let result = import_camera_files(&project.project_path, card.path().to_str().unwrap(), "Camera 1", None).unwrap();
        assert_eq!(result.imported, 2);
        let dest = PathBuf::from(&project.project_path).join("Originals/Camera 1");
        let original = fs::read(dest.join("IMG_0001.jpg")).unwrap();
        let suffixed = fs::read(dest.join("IMG_0001_001.jpg")).unwrap();
        let mut contents = [original, suffixed];
        contents.sort();
        assert_eq!(contents[0], b"different photo bytes!".to_vec());
        assert_eq!(contents[1], b"first photo bytes".to_vec());
    }

    /// Every common RAW extension is accepted, in upper and lower case.
    #[test]
    fn raw_extensions_accepted_any_case() {
        let extensions = [
            "arw", "srf", "sr2", "cr2", "cr3", "crw", "nef", "nrw", "raf", "orf", "rw2",
            "rwl", "pef", "ptx", "srw", "dng", "3fr", "fff", "iiq", "x3f", "mrw", "erf",
            "mef", "mos", "dcr", "k25", "kdc", "gpr", "bay", "raw",
        ];
        let base = TestDir::new("rawext-base");
        let card = TestDir::new("rawext-card");
        let project = make_project(&base);
        for (i, ext) in extensions.iter().enumerate() {
            let variant = if i % 2 == 0 { ext.to_string() } else { ext.to_uppercase() };
            write_file(card.path(), &format!("DCIM/IMG_{i:04}.{variant}"), &[i as u8; 64]);
        }
        let result = import_camera_files(
            &project.project_path,
            card.path().to_str().unwrap(),
            "Camera 1",
            None,
        )
        .unwrap();
        assert_eq!(result.imported, extensions.len());
        assert_eq!(result.skipped_unsupported, 0);
    }

    /// FND-009: unsupported files are skipped and reported, not copied.
    #[test]
    fn fnd_009_unsupported_skipped() {
        let base = TestDir::new("fnd009-base");
        let card = TestDir::new("fnd009-card");
        let project = make_project(&base);
        write_file(card.path(), "notes.txt", b"not a photo");
        write_file(card.path(), "clip.mp4", b"not a photo either");
        write_file(card.path(), "real.cr2", b"fake raw bytes");

        let result = import_camera_files(&project.project_path, card.path().to_str().unwrap(), "Camera 1", None).unwrap();
        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped_unsupported, 2);
        assert!(!PathBuf::from(&project.project_path).join("Originals/Camera 1/notes.txt").exists());
    }

    /// Import rejects nested project/source folders (recursive self-import guard).
    #[test]
    fn import_rejects_nested_folders() {
        let base = TestDir::new("nested-base");
        let project = make_project(&base);
        // Source inside the project.
        let inside = PathBuf::from(&project.project_path).join("Mobile Edits");
        let result = import_camera_files(&project.project_path, inside.to_str().unwrap(), "Camera 1", None);
        assert!(result.is_err());
        // Project inside the source.
        let result = import_camera_files(&project.project_path, base.path().to_str().unwrap(), "Camera 1", None);
        assert!(result.is_err());
    }


    fn fake_jpeg() -> Vec<u8> {
        // Minimal valid JPEG header bytes; the fake corrector only copies them.
        vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46]
    }

    fn fake_correct_ok(src: &Path, dst: &Path, _q: u8) -> Result<(), String> {
        fs::copy(src, dst).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn fake_correct_fail(_src: &Path, _dst: &Path, _q: u8) -> Result<(), String> {
        Err("engine exploded".to_string())
    }

    fn project_with_photos() -> (TestDir, TestDir, CreateProjectResult) {
        let base = TestDir::new("ac-base");
        let card = TestDir::new("ac-card");
        let project = make_project(&base);
        write_file(card.path(), "DCIM/IMG_0001.cr2", &[7u8; 1024]);
        write_file(card.path(), "DCIM/IMG_0002.cr2", &[9u8; 2048]);
        import_camera_files(
            &project.project_path,
            card.path().to_str().unwrap(),
            "Camera 1",
            None,
        )
        .unwrap();
        (base, card, project)
    }

    /// Auto-correction writes new JPEGs into Export/<camera>; originals untouched.
    #[test]
    fn auto_correct_processes_and_preserves_originals() {
        let (_base, card, project) = project_with_photos();
        let src = card.path().join("DCIM/IMG_0001.cr2");
        let src_hash_before = sha256_file(&src).unwrap();
        let original = PathBuf::from(&project.project_path).join("Originals/Camera 1/IMG_0001.cr2");
        let orig_hash_before = sha256_file(&original).unwrap();

        let report = run_auto_correct(
            &project.project_path,
            "Camera 1",
            None,
            90,
            &fake_correct_ok,
            &|_| {},
        )
        .unwrap();

        assert_eq!(report.total, 2);
        assert_eq!(report.processed, 2);
        assert!(report.failed.is_empty());
        assert!(PathBuf::from(&report.output_dir).join("IMG_0001.jpg").exists());
        assert!(PathBuf::from(&report.output_dir).join("IMG_0002.jpg").exists());
        // Originals untouched.
        assert_eq!(src_hash_before, sha256_file(&src).unwrap());
        assert_eq!(orig_hash_before, sha256_file(&original).unwrap());
    }

    /// Re-running with the same settings skips finished work.
    #[test]
    fn auto_correct_rerun_skips_done() {
        let (_base, _card, project) = project_with_photos();
        let first = run_auto_correct(&project.project_path, "Camera 1", None, 90, &fake_correct_ok, &|_| {}).unwrap();
        let second = run_auto_correct(&project.project_path, "Camera 1", None, 90, &fake_correct_ok, &|_| {}).unwrap();
        assert_eq!(first.processed, 2);
        assert_eq!(second.processed, 0);
        assert_eq!(second.already_done, 2);
    }

    /// A different quality setting is a new job, not a duplicate.
    #[test]
    fn auto_correct_new_quality_reprocesses() {
        let (_base, _card, project) = project_with_photos();
        run_auto_correct(&project.project_path, "Camera 1", None, 90, &fake_correct_ok, &|_| {}).unwrap();
        let rerun = run_auto_correct(&project.project_path, "Camera 1", None, 80, &fake_correct_ok, &|_| {}).unwrap();
        assert_eq!(rerun.processed, 2);
        // Old outputs were not overwritten; new ones got unique names.
        let dir = PathBuf::from(&project.project_path).join("Export/Camera 1");
        let jpgs: Vec<_> = fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).collect();
        assert_eq!(jpgs.len(), 4);
    }

    /// Engine failures are collected per file and leave no output behind.
    #[test]
    fn auto_correct_collects_failures() {
        let (_base, _card, project) = project_with_photos();
        let report = run_auto_correct(&project.project_path, "Camera 1", None, 90, &fake_correct_fail, &|_| {}).unwrap();
        assert_eq!(report.processed, 0);
        assert_eq!(report.failed.len(), 2);
        assert!(report.failed[0].error.contains("engine exploded"));
        let dir = PathBuf::from(&project.project_path).join("Export/Camera 1");
        let leftover: Vec<_> = fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).collect();
        assert!(leftover.is_empty());
        // Failure is not recorded as done: a retry with a working engine processes it.
        let retry = run_auto_correct(&project.project_path, "Camera 1", None, 90, &fake_correct_ok, &|_| {}).unwrap();
        assert_eq!(retry.processed, 2);
    }

    /// Output folder can be picked freely; it must not sit inside Originals.
    #[test]
    fn auto_correct_custom_output_and_originals_guard() {
        let (_base, _card, project) = project_with_photos();
        let picked = TestDir::new("ac-output");
        let report = run_auto_correct(
            &project.project_path,
            "Camera 1",
            Some(picked.path().to_str().unwrap()),
            90,
            &fake_correct_ok,
            &|_| {},
        )
        .unwrap();
        assert!(report.output_dir.starts_with(picked.path().to_str().unwrap()));
        assert!(PathBuf::from(&report.output_dir).join("IMG_0001.jpg").exists());

        let bad = run_auto_correct(
            &project.project_path,
            "Camera 1",
            Some(PathBuf::from(&project.project_path).join("Originals").to_str().unwrap()),
            90,
            &fake_correct_ok,
            &|_| {},
        );
        assert!(bad.is_err());
    }

    /// Import into a user-picked destination keeps Originals/<camera> structure,
    /// records the absolute path, and leaves the project defaults intact.
    #[test]
    fn import_to_picked_destination() {
        let base = TestDir::new("dest-base");
        let card = TestDir::new("dest-card");
        let picked = TestDir::new("dest-picked");
        let project = make_project(&base);
        write_file(card.path(), "DCIM/IMG_0001.cr2", &[5u8; 512]);

        let result = import_camera_files(
            &project.project_path,
            card.path().to_str().unwrap(),
            "Camera 1",
            Some(picked.path().to_str().unwrap()),
        )
        .unwrap();
        assert_eq!(result.imported, 1);
        let landed = picked.path().join("Originals/Camera 1/IMG_0001.cr2");
        assert!(landed.exists(), "file must land under picked/Originals/Camera 1");
        // Default project Originals stays empty.
        assert!(fs::read_dir(PathBuf::from(&project.project_path).join("Originals/Camera 1"))
            .unwrap().next().is_none());
        // Auto-correct resolves the moved file through the recorded absolute path.
        let report = run_auto_correct(&project.project_path, "Camera 1", None, 90, &fake_correct_ok, &|_| {}).unwrap();
        assert_eq!(report.processed, 1);
    }

    /// Import rejects anything but Camera 1 / Camera 2.
    #[test]
    fn import_rejects_unknown_camera() {
        let base = TestDir::new("camera-base");
        let card = TestDir::new("camera-card");
        let project = make_project(&base);
        let result = import_camera_files(&project.project_path, card.path().to_str().unwrap(), "Camera 3", None);
        assert!(result.is_err());
    }

    /// Import fails clearly when the folder is not a Frame Sense project.
    #[test]
    fn import_rejects_non_project_folder() {
        let base = TestDir::new("notproj-base");
        let card = TestDir::new("notproj-card");
        write_file(card.path(), "A.jpg", b"bytes");
        let result = import_camera_files(base.path().to_str().unwrap(), card.path().to_str().unwrap(), "Camera 1", None);
        assert!(result.is_err());
    }
}
