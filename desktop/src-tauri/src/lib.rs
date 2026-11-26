mod classify;
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
async fn import_camera_files(
    project_path: String,
    source_path: String,
    camera_id: String,
    destination_dir: Option<String>,
) -> Result<core::ImportResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        core::import_camera_files(
            &project_path,
            &source_path,
            &camera_id,
            destination_dir.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("import failed to run: {e}"))?
}

// ---------------------------------------------------------------------------
// RAW engine (framesense_raw.dll) - loaded at runtime, LGPL component stays
// in its own replaceable binary.
// ---------------------------------------------------------------------------

type FnAutoCorrect = unsafe extern "C" fn(*const std::ffi::c_char, *const std::ffi::c_char, u8) -> i32;
type FnAutoCorrectLook =
    unsafe extern "C" fn(*const std::ffi::c_char, *const std::ffi::c_char, u8, u8) -> i32;
type FnLastError = unsafe extern "C" fn() -> *mut std::ffi::c_char;
type FnFreeString = unsafe extern "C" fn(*mut std::ffi::c_char);
type FnVersion = unsafe extern "C" fn() -> *mut std::ffi::c_char;

struct RawEngine {
    _lib: libloading::Library,
    auto_correct: FnAutoCorrect,
    auto_correct_look: Option<FnAutoCorrectLook>,
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
        // Optional: session-look-aware entry point (engine >= 0.4.1).
        let auto_correct_look = lib
            .get::<FnAutoCorrectLook>(b"fs_raw_auto_correct_file_look\0")
            .ok()
            .map(|s| *s);
        Ok(RawEngine {
            _lib: lib,
            auto_correct,
            auto_correct_look,
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
    engine_correct_look(src, dst, quality, 1)
}

fn engine_correct_look(src: &Path, dst: &Path, quality: u8, look: u8) -> Result<(), String> {
    let engine = engine()?;
    let src_c = CString::new(src.to_string_lossy().as_bytes())
        .map_err(|_| "source path is not representable".to_string())?;
    let dst_c = CString::new(dst.to_string_lossy().as_bytes())
        .map_err(|_| "destination path is not representable".to_string())?;
    let code = unsafe {
        match engine.auto_correct_look {
            Some(f) => f(src_c.as_ptr(), dst_c.as_ptr(), quality, look),
            None => (engine.auto_correct)(src_c.as_ptr(), dst_c.as_ptr(), quality),
        }
    };
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

// Processing control shared by both auto-correct commands.
static PROCESSING_CONTROL: std::sync::Mutex<u8> = std::sync::Mutex::new(0); // 0 run, 1 pause, 2 stop

fn processing_control() -> core::ControlAction {
    match PROCESSING_CONTROL.lock().map(|v| *v).unwrap_or(0) {
        1 => core::ControlAction::Pause,
        2 => core::ControlAction::Stop,
        _ => core::ControlAction::Continue,
    }
}

fn reset_processing_control() {
    if let Ok(mut v) = PROCESSING_CONTROL.lock() {
        *v = 0;
    }
}

#[command]
fn set_processing_control(command: String) -> Result<(), String> {
    let value = match command.as_str() {
        "pause" => 1,
        "resume" => 0,
        "stop" => 2,
        _ => return Err(format!("unknown control command: {command}")),
    };
    if let Ok(mut v) = PROCESSING_CONTROL.lock() {
        *v = value;
    }
    Ok(())
}

// Heavy work runs on a blocking thread, never the UI thread: the window
// stays responsive and Pause/Stop work while hundreds of photos process.
#[command]
async fn auto_correct_folder(
    folder_path: String,
    output_dir: Option<String>,
    quality: Option<u8>,
    look: Option<String>,
    bw: Option<bool>,
    on_progress: tauri::ipc::Channel<core::AutoCorrectProgress>,
) -> Result<core::AutoCorrectReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        engine()?;
        reset_processing_control();
        let quality = quality.unwrap_or(core::DEFAULT_JPEG_QUALITY);
        let look_num = look_code(look.as_deref());
        let bw = bw.unwrap_or(true);
        let correct = move |src: &Path, dst: &Path, q: u8| {
            engine_correct_look(src, dst, q, look_num)
        };
        core::run_auto_correct_folder(
            &folder_path,
            output_dir.as_deref(),
            quality,
            &correct,
            &|p| {
                let _ = on_progress.send(p);
            },
            &processing_control,
            &classify::classify_output,
            &format!("q{quality}:look{look_num}:bw{bw}"),
            bw,
        )
    })
    .await
    .map_err(|e| format!("processing failed to run: {e}"))?
}

/// Session look name to the engine's look code.
fn look_code(look: Option<&str>) -> u8 {
    match look {
        Some("natural") => 0,
        Some("golden") => 2,
        _ => 1, // warm is the default
    }
}

#[command]
async fn auto_correct_camera(
    project_path: String,
    camera_id: String,
    output_dir: Option<String>,
    quality: Option<u8>,
    look: Option<String>,
    bw: Option<bool>,
    on_progress: tauri::ipc::Channel<core::AutoCorrectProgress>,
) -> Result<core::AutoCorrectReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // Surface a missing engine before touching the project.
        engine()?;
        reset_processing_control();
        let quality = quality.unwrap_or(core::DEFAULT_JPEG_QUALITY);
        let look_num = look_code(look.as_deref());
        let bw = bw.unwrap_or(true);
        let correct = move |src: &Path, dst: &Path, q: u8| {
            engine_correct_look(src, dst, q, look_num)
        };
        core::run_auto_correct(
            &project_path,
            &camera_id,
            output_dir.as_deref(),
            quality,
            &correct,
            &|p| {
                let _ = on_progress.send(p);
            },
            &processing_control,
            &classify::classify_output,
            &format!("q{quality}:look{look_num}:bw{bw}"),
            bw,
        )
    })
    .await
    .map_err(|e| format!("processing failed to run: {e}"))?
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            create_project,
            import_camera_files,
            auto_correct_camera,
            auto_correct_folder,
            set_processing_control,
            raw_engine_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running Frame Sense");
}
