mod core;

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::command;

#[command]
fn create_project(
    base_directory: String,
    client_name: String,
    event_type: String,
    event_date: String,
) -> Result<core::CreateProjectResult, String> {
    core::create_project(&base_directory, &client_name, &event_type, &event_date)
}

#[command]
fn import_camera_files(
    project_path: String,
    source_path: String,
    camera_id: String,
    destination_dir: Option<String>,
) -> Result<core::ImportResult, String> {
    core::import_camera_files(
        &project_path,
        &source_path,
        &camera_id,
        destination_dir.as_deref(),
    )
}

// ---------------------------------------------------------------------------
// RAW engine (framesense_raw.dll) - loaded at runtime, LGPL component stays
// in its own replaceable binary.
// ---------------------------------------------------------------------------

type FnAutoCorrect = unsafe extern "C" fn(*const std::ffi::c_char, *const std::ffi::c_char, u8) -> i32;
type FnLastError = unsafe extern "C" fn() -> *mut std::ffi::c_char;
type FnFreeString = unsafe extern "C" fn(*mut std::ffi::c_char);
type FnVersion = unsafe extern "C" fn() -> *mut std::ffi::c_char;

struct RawEngine {
    _lib: libloading::Library,
    auto_correct: FnAutoCorrect,
    last_error: FnLastError,
    free_string: FnFreeString,
    version: FnVersion,
}

// The function pointers are plain C ABI entry points; the library handle
// lives inside the struct so they cannot outlive it.
unsafe impl Send for RawEngine {}
unsafe impl Sync for RawEngine {}

fn engine_library_path() -> Result<PathBuf, String> {
    if let Ok(custom) = std::env::var("FRAME_SENSE_RAW_ENGINE") {
        let path = PathBuf::from(custom);
        if path.is_file() {
            return Ok(path);
        }
    }
    let file_name = if cfg!(windows) {
        "framesense_raw.dll"
    } else if cfg!(target_os = "macos") {
        "libframesense_raw.dylib"
    } else {
        "libframesense_raw.so"
    };
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    if let Some(dir) = exe_dir {
        let candidate = dir.join(file_name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "RAW engine ({file_name}) was not found next to the app. Auto-correction is unavailable; import still works."
    ))
}

fn load_engine() -> Result<RawEngine, String> {
    let path = engine_library_path()?;
    unsafe {
        let lib = libloading::Library::new(&path)
            .map_err(|e| format!("RAW engine failed to load: {e}"))?;
        let auto_correct = *lib
            .get::<FnAutoCorrect>(b"fs_raw_auto_correct_file\0")
            .map_err(|e| format!("RAW engine is missing its entry point: {e}"))?;
        let last_error = *lib
            .get::<FnLastError>(b"fs_raw_last_error\0")
            .map_err(|e| format!("RAW engine is incomplete: {e}"))?;
        let free_string = *lib
            .get::<FnFreeString>(b"fs_raw_free_string\0")
            .map_err(|e| format!("RAW engine is incomplete: {e}"))?;
        let version = *lib
            .get::<FnVersion>(b"fs_raw_engine_version\0")
            .map_err(|e| format!("RAW engine is incomplete: {e}"))?;
        Ok(RawEngine {
            _lib: lib,
            auto_correct,
            last_error,
            free_string,
            version,
        })
    }
}

static ENGINE: OnceLock<Result<RawEngine, String>> = OnceLock::new();

fn engine() -> Result<&'static RawEngine, String> {
    ENGINE
        .get_or_init(load_engine)
        .as_ref()
        .map_err(|e| e.clone())
}

fn engine_string(engine: &RawEngine, ptr: *mut std::ffi::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe {
        let text = std::ffi::CStr::from_ptr(ptr).to_string_lossy().to_string();
        (engine.free_string)(ptr);
        text
    }
}

fn engine_correct(src: &Path, dst: &Path, quality: u8) -> Result<(), String> {
    let engine = engine()?;
    let src_c = CString::new(src.to_string_lossy().as_bytes())
        .map_err(|_| "source path is not representable".to_string())?;
    let dst_c = CString::new(dst.to_string_lossy().as_bytes())
        .map_err(|_| "destination path is not representable".to_string())?;
    let code = unsafe { (engine.auto_correct)(src_c.as_ptr(), dst_c.as_ptr(), quality) };
    if code == 0 {
        Ok(())
    } else {
        let message = unsafe { engine_string(engine, (engine.last_error)()) };
        Err(if message.is_empty() {
            format!("RAW engine error {code}")
        } else {
            message
        })
    }
}

#[derive(serde::Serialize)]
struct RawEngineStatus {
    available: bool,
    version: Option<String>,
    path: Option<String>,
    error: Option<String>,
}

#[command]
fn raw_engine_status() -> RawEngineStatus {
    match engine() {
        Ok(engine) => {
            let version = unsafe { engine_string(engine, (engine.version)()) };
            RawEngineStatus {
                available: true,
                version: Some(version),
                path: engine_library_path().ok().map(|p| p.to_string_lossy().to_string()),
                error: None,
            }
        }
        Err(e) => RawEngineStatus {
            available: false,
            version: None,
            path: None,
            error: Some(e),
        },
    }
}

#[command]
fn auto_correct_camera(
    project_path: String,
    camera_id: String,
    output_dir: Option<String>,
    quality: Option<u8>,
    on_progress: tauri::ipc::Channel<core::AutoCorrectProgress>,
) -> Result<core::AutoCorrectReport, String> {
    // Surface a missing engine before touching the project.
    engine()?;
    let quality = quality.unwrap_or(core::DEFAULT_JPEG_QUALITY);
    core::run_auto_correct(
        &project_path,
        &camera_id,
        output_dir.as_deref(),
        quality,
        &engine_correct,
        &|p| {
            let _ = on_progress.send(p);
        },
    )
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            create_project,
            import_camera_files,
            auto_correct_camera,
            raw_engine_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running Frame Sense");
}
