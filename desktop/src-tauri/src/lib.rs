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
#[allow(clippy::type_complexity)]
type FnAutoCorrectV2 = unsafe extern "C" fn(
    *const std::ffi::c_char,
    *const std::ffi::c_char,
    u8,
    u8,
    u8,
    *const std::ffi::c_char,
    *const std::ffi::c_char,
) -> i32;
#[allow(clippy::type_complexity)]
type FnProbeLook = unsafe extern "C" fn(*const std::ffi::c_char) -> *mut std::ffi::c_char;
type FnEyesClosed = unsafe extern "C" fn(*const std::ffi::c_char, *const std::ffi::c_char, *const std::ffi::c_char) -> i32;
type FnPreviewJpeg = unsafe extern "C" fn(
    *const std::ffi::c_char,
    u32,
    u8,
    u8,
    *const std::ffi::c_char,
    *const std::ffi::c_char,
) -> *mut std::ffi::c_char;
type FnSetAdjust = unsafe extern "C" fn(f32, f32, f32, f32, f32);
type FnSetAiLook = unsafe extern "C" fn(u8, f32, *const std::ffi::c_char, *const std::ffi::c_char);
type FnLastError = unsafe extern "C" fn() -> *mut std::ffi::c_char;
type FnFreeString = unsafe extern "C" fn(*mut std::ffi::c_char);
type FnVersion = unsafe extern "C" fn() -> *mut std::ffi::c_char;

struct RawEngine {
    _lib: libloading::Library,
    auto_correct: FnAutoCorrect,
    auto_correct_look: Option<FnAutoCorrectLook>,
    auto_correct_v2: Option<FnAutoCorrectV2>,
    preview_jpeg: Option<FnPreviewJpeg>,
    preview_clip: Option<unsafe extern "C" fn() -> f32>,
    probe_look: Option<FnProbeLook>,
    eyes_closed: Option<FnEyesClosed>,
    set_adjust: Option<FnSetAdjust>,
    set_ai_look: Option<FnSetAiLook>,
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
        // Optional: retouch + preview entry points (engine >= 0.5.0).
        let auto_correct_v2 = lib
            .get::<FnAutoCorrectV2>(b"fs_raw_auto_correct_file_v2\0")
            .ok()
            .map(|s| *s);
        let preview_jpeg = lib
            .get::<FnPreviewJpeg>(b"fs_raw_preview_jpeg\0")
            .ok()
            .map(|s| *s);
        let probe_look = lib.get::<FnProbeLook>(b"fs_raw_probe_look\0").ok().map(|s| *s);
        let eyes_closed = lib.get::<FnEyesClosed>(b"fs_raw_eyes_closed\0").ok().map(|s| *s);
        let set_adjust = lib.get::<FnSetAdjust>(b"fs_raw_set_adjust\0").ok().map(|s| *s);
        let preview_clip = lib.get::<unsafe extern "C" fn() -> f32>(b"fs_raw_preview_clip\0").ok().map(|s| *s);
        let set_ai_look = lib.get::<FnSetAiLook>(b"fs_raw_set_ai_look\0").ok().map(|s| *s);
        Ok(RawEngine {
            _lib: lib,
            set_adjust,
            set_ai_look,
            auto_correct,
            auto_correct_look,
            auto_correct_v2,
            preview_jpeg,
            preview_clip,
            probe_look,
            eyes_closed,
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

/// Folder next to the engine DLL: onnxruntime and the model files are
/// bundled there too (Tauri resources land next to the exe on Windows).
fn engine_dir() -> Result<PathBuf, String> {
    let dll = engine_library_path()?;
    dll.parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "RAW engine folder is unavailable".to_string())
}

fn ort_library_path() -> Result<PathBuf, String> {
    let file_name = if cfg!(windows) {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    };
    let path = engine_dir()?.join(file_name);
    if path.is_file() {
        Ok(path)
    } else {
        Err("Face retouch is unavailable: onnxruntime was not found next to the app.".to_string())
    }
}

fn face_model_dir() -> Result<PathBuf, String> {
    let dir = engine_dir()?.join("models");
    if dir.join("face_mesh.onnx").is_file() && dir.join("seeta_fd_frontal_v1.0.bin").is_file() {
        Ok(dir)
    } else {
        Err("Face retouch is unavailable: the face models are missing next to the app.".to_string())
    }
}

/// Sort class for a corrected photo: the blur/face-count classifier, plus
/// closed or lowered eyes on the main faces -> Rejected (review-only copy).
/// If the eye model or engine support is missing, only blur applies.
fn classify_with_eyes(path: &Path) -> classify::SortClass {
    let base = classify::classify_output(path);
    if base == classify::SortClass::Rejected {
        return base;
    }
    let check = || -> Option<bool> {
        let engine = engine().ok()?;
        let f = engine.eyes_closed?;
        let dir = engine_dir().ok()?.join("models");
        if !dir.join("open_closed_eye.onnx").is_file() {
            return None;
        }
        let ort = ort_library_path().ok()?;
        dir.join("face_mesh.onnx").is_file().then_some(())?;
        let p = CString::new(path.to_string_lossy().as_bytes()).ok()?;
        let o = CString::new(ort.to_string_lossy().as_bytes()).ok()?;
        let m = CString::new(dir.to_string_lossy().as_bytes()).ok()?;
        match unsafe { f(p.as_ptr(), o.as_ptr(), m.as_ptr()) } {
            1 => Some(true),
            0 => Some(false),
            _ => None,
        }
    };
    if check() == Some(true) {
        classify::SortClass::Rejected
    } else {
        base
    }
}

fn engine_correct_look_v2(
    src: &Path,
    dst: &Path,
    quality: u8,
    look: u8,
    retouch: bool,
) -> Result<(), String> {
    let engine = engine()?;
    let v2 = match engine.auto_correct_v2 {
        Some(f) => f,
        None => return engine_correct_look(src, dst, quality, look),
    };
    let src_c = CString::new(src.to_string_lossy().as_bytes())
        .map_err(|_| "source path is not representable".to_string())?;
    let dst_c = CString::new(dst.to_string_lossy().as_bytes())
        .map_err(|_| "destination path is not representable".to_string())?;
    // Models are needed for retouch AND for auto-upright (face-based
    // rotation fix). Without retouch, missing models just skip upright.
    let models_dir = match face_model_dir() {
        Ok(d) => Some(d),
        Err(e) if retouch => return Err(e),
        Err(_) => None,
    };
    if !retouch && models_dir.is_none() {
        return engine_correct_look(src, dst, quality, look);
    }
    let ort_c = if retouch {
        Some(
            CString::new(ort_library_path()?.to_string_lossy().as_bytes())
                .map_err(|_| "onnxruntime path is not representable".to_string())?,
        )
    } else {
        None
    };
    let models_c = CString::new(models_dir.unwrap().to_string_lossy().as_bytes())
        .map_err(|_| "model path is not representable".to_string())?;
    let code = unsafe {
        v2(
            src_c.as_ptr(),
            dst_c.as_ptr(),
            quality,
            look,
            if retouch { 1 } else { 0 },
            ort_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
            models_c.as_ptr(),
        )
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
/// Describes the sliders and AI look in force, so changing them (or the app
/// version) makes a re-run redo photos instead of skipping old results.
static EDIT_STATE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
fn edit_state_tag() -> String {
    format!("v{}:{}", env!("CARGO_PKG_VERSION"), EDIT_STATE.lock().map(|s| s.clone()).unwrap_or_default())
}
static ADJ_TAG: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
static AI_TAG: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
fn refresh_edit_state() {
    let a = ADJ_TAG.lock().map(|s| s.clone()).unwrap_or_default();
    let b = AI_TAG.lock().map(|s| s.clone()).unwrap_or_default();
    if let Ok(mut s) = EDIT_STATE.lock() { *s = format!("{a}{b}"); }
}
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
/// Optional sliders (each -1..1): contrast, brightness, warmth, fade, saturation.
#[command]
fn set_adjust(contrast: f32, brightness: f32, warmth: f32, fade: f32, saturation: f32) -> Result<(), String> {
    let e = engine()?;
    if let Ok(mut t) = ADJ_TAG.lock() { *t = format!("adj{contrast:.2},{brightness:.2},{warmth:.2},{fade:.2},{saturation:.2};"); }
    refresh_edit_state();
    if let Some(f) = e.set_adjust {
        unsafe { f(contrast, brightness, warmth, fade, saturation) };
    }
    Ok(())
}

/// Optional AI look (3D LUT network, research-licensed weights). Off by default.
#[command]
fn set_ai_look(enabled: bool, strength: f32) -> Result<(), String> {
    let e = engine()?;
    if let Ok(mut t) = AI_TAG.lock() { *t = if enabled { format!("ai{strength:.2};") } else { String::new() }; }
    refresh_edit_state();
    let f = e.set_ai_look.ok_or_else(|| "This engine build has no AI look.".to_string())?;
    let dir = engine_dir()?.join("models");
    if enabled && !(dir.join("lut3_classifier.onnx").is_file() && dir.join("lut3_luts.bin").is_file()) {
        return Err("The AI look model files are missing next to the app.".to_string());
    }
    let ort = ort_library_path()?;
    let o = CString::new(ort.to_string_lossy().as_bytes()).map_err(|_| "bad path".to_string())?;
    let m = CString::new(dir.to_string_lossy().as_bytes()).map_err(|_| "bad path".to_string())?;
    unsafe { f(if enabled { 1 } else { 0 }, strength, o.as_ptr(), m.as_ptr()) };
    Ok(())
}

#[command]
async fn auto_correct_folder(
    folder_path: String,
    output_dir: Option<String>,
    quality: Option<u8>,
    look: Option<String>,
    bw: Option<bool>,
    retouch: Option<bool>,
    on_progress: tauri::ipc::Channel<core::AutoCorrectProgress>,
) -> Result<core::AutoCorrectReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        engine()?;
        reset_processing_control();
        let quality = quality.unwrap_or(core::DEFAULT_JPEG_QUALITY);
        let look_num = look_code(look.as_deref());
        let want_retouch = retouch.unwrap_or(false);
        let bw = bw.unwrap_or(true);
        let retouch = retouch.unwrap_or(false);
        let correct = move |src: &Path, dst: &Path, q: u8| {
            engine_correct_look_v2(src, dst, q, look_num, retouch)
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
            &classify_with_eyes,
            &format!("q{quality}:look{look_num}:bw{bw}:rt{retouch}:{}", edit_state_tag()),
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
        Some("peach") => 3,
        Some("amber") => 4,
        Some("airy") => 5,
        Some("cinematic") => 6,
        Some("cool") => 7,
        Some("punchy") => 8,
        Some("warm") => 1,
        Some(x) if x.starts_with("style") => 10 + x[5..].parse::<u8>().unwrap_or(0).min(23),
        _ => 9, // auto (per-photo) is the default
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
    retouch: Option<bool>,
    on_progress: tauri::ipc::Channel<core::AutoCorrectProgress>,
) -> Result<core::AutoCorrectReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // Surface a missing engine before touching the project.
        engine()?;
        reset_processing_control();
        let quality = quality.unwrap_or(core::DEFAULT_JPEG_QUALITY);
        let look_num = look_code(look.as_deref());
        let want_retouch = retouch.unwrap_or(false);
        let bw = bw.unwrap_or(true);
        let retouch = retouch.unwrap_or(false);
        let correct = move |src: &Path, dst: &Path, q: u8| {
            engine_correct_look_v2(src, dst, q, look_num, retouch)
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
            &classify_with_eyes,
            &format!("q{quality}:look{look_num}:bw{bw}:rt{retouch}:{}", edit_state_tag()),
            bw,
        )
    })
    .await
    .map_err(|e| format!("processing failed to run: {e}"))?
}

/// Suggested session tone for a folder: probes up to 12 sample photos with
/// the engine's deterministic brightness/colour heuristic and votes.
#[derive(serde::Serialize)]
struct ToneSuggestion {
    tone: String,
    reason: String,
    probed: usize,
}

#[command]
async fn suggest_session_tone(folder_path: String) -> Result<ToneSuggestion, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let engine = engine()?;
        let probe = engine
            .probe_look
            .ok_or_else(|| "This engine build has no tone suggestion support.".to_string())?;
        let root = PathBuf::from(&folder_path);
        if !root.is_dir() {
            return Err("folder not found".to_string());
        }
        let mut files: Vec<PathBuf> = walkdir::WalkDir::new(&root)
            .into_iter()
            .filter_map(|e| e.ok())
            .map(|e| e.into_path())
            .filter(|p| p.is_file() && core::is_supported_photo(p))
            .collect();
        files.sort();
        if files.is_empty() {
            return Err("no photos found in that folder".to_string());
        }
        let sample_n = 12usize.min(files.len());
        let step = (files.len() / sample_n).max(1);
        let mut votes = [0usize; 7];
        let mut reasons: std::collections::HashMap<u8, String> = std::collections::HashMap::new();
        let mut probed = 0usize;
        for i in (0..files.len()).step_by(step).take(sample_n) {
            let src_c = match CString::new(files[i].to_string_lossy().as_bytes()) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let ptr = unsafe { probe(src_c.as_ptr()) };
            if ptr.is_null() {
                continue;
            }
            let json = unsafe { engine_string(engine, ptr) };
            let look = json
                .split("\"look\":")
                .nth(1)
                .and_then(|t| t.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u8>().ok())
                .unwrap_or(1);
            let reason = json
                .split("\"reason\":\"")
                .nth(1)
                .and_then(|t| t.split('\"').next())
                .unwrap_or("")
                .to_string();
            if (look as usize) < votes.len() {
                votes[look as usize] += 1;
                reasons.insert(look, reason);
                probed += 1;
            }
        }
        if probed == 0 {
            return Err("could not probe any photos".to_string());
        }
        let best = votes
            .iter()
            .enumerate()
            .max_by_key(|(_, v)| *v)
            .map(|(i, _)| i as u8)
            .unwrap_or(1);
        let tone = match best {
            0 => "natural",
            2 => "golden",
            3 => "peach",
            4 => "amber",
            5 => "airy",
            6 => "cinematic",
            7 => "cool",
            8 => "punchy",
            9 => "auto",
            _ => "warm",
        };
        Ok(ToneSuggestion {
            tone: tone.to_string(),
            reason: reasons.get(&best).cloned().unwrap_or_default(),
            probed,
        })
    })
    .await
    .map_err(|e| format!("tone suggestion failed to run: {e}"))?
}

/// Session preview on one photo: [baseline, chosen] as base64 JPEGs.
/// Baseline is the Natural tone without retouch; chosen is the selected
/// session tone, with face retouch when `retouch` is true.
#[command]
async fn retouch_preview(
    photo_path: String,
    look: Option<String>,
    retouch: Option<bool>,
) -> Result<[String; 4], String> {
    tauri::async_runtime::spawn_blocking(move || {
        let engine = engine()?;
        let preview = engine
            .preview_jpeg
            .ok_or_else(|| "This engine build has no preview support.".to_string())?;
        let look_num = look_code(look.as_deref());
        let want_retouch = retouch.unwrap_or(false);
        let src = PathBuf::from(&photo_path);
        let src_c = CString::new(photo_path.as_bytes())
            .map_err(|_| "photo path is not representable".to_string())?;
        if !src.is_file() {
            return Err("photo not found".to_string());
        }
        let run_one = |look: u8, retouch: u8| -> Result<String, String> {
            let (ort_c, models_c) = if retouch == 1 {
                (
                    Some(CString::new(ort_library_path()?.to_string_lossy().as_bytes()).map_err(|_| "onnxruntime path is not representable".to_string())?),
                    Some(CString::new(face_model_dir()?.to_string_lossy().as_bytes()).map_err(|_| "model path is not representable".to_string())?),
                )
            } else {
                // models still passed (when present) for auto-upright
                let m = face_model_dir()
                    .ok()
                    .and_then(|d| CString::new(d.to_string_lossy().as_bytes()).ok());
                (None, m)
            };
            let ptr = unsafe {
                preview(
                    src_c.as_ptr(),
                    1400,
                    look,
                    retouch,
                    ort_c.as_ref().map(|c| c.as_ptr()).unwrap_or(std::ptr::null()),
                    models_c.as_ref().map(|c| c.as_ptr()).unwrap_or(std::ptr::null()),
                )
            };
            if ptr.is_null() {
                let message = unsafe { engine_string(engine, (engine.last_error)()) };
                Err(if message.is_empty() { "preview failed".to_string() } else { message })
            } else {
                Ok(unsafe { engine_string(engine, ptr) })
            }
        };
        let clip = || engine.preview_clip.map(|f| unsafe { f() }).unwrap_or(-1.0);
        // 255 = plain developed photo with NO correction: the real "before".
        let before = run_one(255, 0)?;
        let clip_before = clip();
        let after = run_one(look_num, if want_retouch { 1 } else { 0 })?;
        let clip_after = clip();
        Ok([before, after, format!("{clip_before:.1}"), format!("{clip_after:.1}")])
    })
    .await
    .map_err(|e| format!("preview failed to run: {e}"))?
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            create_project,
            import_camera_files,
            auto_correct_camera,
            auto_correct_folder,
            set_adjust,
            set_processing_control,
            retouch_preview,
            suggest_session_tone,
            raw_engine_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running Frame Sense");
}
