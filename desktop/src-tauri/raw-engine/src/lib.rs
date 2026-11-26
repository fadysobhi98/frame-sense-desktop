//! Frame Sense RAW engine.
//!
//! Decodes camera RAW files (via rawler, LGPL-2.1) and everyday images,
//! applies the Smart Basic auto-correction, and writes an 8-bit sRGB JPEG.
//! Exposed as a small C ABI so the main app can load this as
//! framesense_raw.dll at runtime; also usable as a plain Rust crate.

pub mod denoise;
pub mod pipeline;
pub mod retouch;
pub mod straighten;

use image::{DynamicImage, GenericImageView};
use rawler::imgop::develop::RawDevelop;
use std::ffi::{c_char, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Extensions decoded through rawler. Everyday raster formats
/// (jpg/png/webp/heic) go through the `image` crate instead.
pub fn is_raw_extension(ext: &str) -> bool {
    let ext = ext.to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "arw" | "srf" | "sr2" | "cr2" | "cr3" | "crw" | "nef" | "nrw" | "raf" | "orf" | "rw2"
            | "rwl" | "pef" | "ptx" | "srw" | "dng" | "3fr" | "fff" | "iiq" | "x3f" | "mrw"
            | "erf" | "mef" | "mos" | "dcr" | "k25" | "kdc" | "gpr" | "bay" | "raw"
    )
}

pub fn is_supported_photo(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let ext = ext.to_ascii_lowercase();
    is_raw_extension(&ext) || matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
}

/// Apply an EXIF/rawler orientation so the photo displays upright.
fn apply_orientation(image: DynamicImage, orientation: u16) -> DynamicImage {
    match orientation {
        2 => image.fliph(),
        3 => image.rotate180(),
        4 => image.flipv(),
        5 => image.rotate90().fliph(),
        6 => image.rotate90(),
        7 => image.rotate270().fliph(),
        8 => image.rotate270(),
        _ => image,
    }
}

/// EXIF orientation tag read straight from the file's metadata. Works for
/// JPEG and for TIFF-based RAW (NEF, CR2, DNG, ARW ...); None when the tag
/// or the EXIF block is missing or unreadable (e.g. CR3's BMFF container).
fn exif_orientation(path: &Path) -> Option<u16> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
        .map(|v| v as u16)
}

pub fn decode_to_dynamic(path: &Path) -> Result<DynamicImage, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if is_raw_extension(&ext) {
        let raw = rawler::decode_file(path)
            .map_err(|e| format!("RAW decode failed: {e}"))?;
        // rawler reports Normal for several cameras (verified: Nikon NEF
        // ignores the tag) and develop_intermediate does not rotate, so the
        // file's own EXIF orientation tag is the source of truth; rawler is
        // the fallback for containers this parser cannot read (e.g. CR3).
        let orientation =
            exif_orientation(path).unwrap_or_else(|| orientation_to_exif(raw.orientation));
        let develop = RawDevelop::default();
        let intermediate = develop
            .develop_intermediate(&raw)
            .map_err(|e| format!("RAW develop failed: {e}"))?;
        let image = intermediate
            .to_dynamic_image()
            .ok_or_else(|| "RAW developed to an unsupported pixel layout".to_string())?;
        Ok(apply_orientation(image, orientation))
    } else {
        let orientation = exif_orientation(path).unwrap_or(1);
        let image = image::open(path).map_err(|e| format!("image open failed: {e}"))?;
        Ok(apply_orientation(image, orientation))
    }
}

/// rawler's Orientation to the EXIF orientation number space.
fn orientation_to_exif(o: rawler::Orientation) -> u16 {
    use rawler::Orientation::*;
    match o {
        Normal => 1,
        HorizontalFlip => 2,
        Rotate180 => 3,
        VerticalFlip => 4,
        Transpose => 5,
        Rotate90 => 6,
        Transverse => 7,
        Rotate270 => 8,
        Unknown => 1,
    }
}

/// Decode one photo, auto-correct it, and write an 8-bit JPEG at `dst`
/// (parent folder must exist). Returns the JPEG dimensions on success.
pub fn auto_correct_file(src: &Path, dst: &Path, quality: u8) -> Result<(u32, u32), String> {
    auto_correct_file_with_look(src, dst, quality, pipeline::Look::Warm)
}

/// Same as auto_correct_file, with an explicit session look.
pub fn auto_correct_file_with_look(
    src: &Path,
    dst: &Path,
    quality: u8,
    look: pipeline::Look,
) -> Result<(u32, u32), String> {
    let decoded = decode_to_dynamic(src)?;
    let corrected = pipeline::auto_correct(&decoded, look);
    let straightened = straighten::auto_straighten(&corrected.to_rgb32f());
    let corrected = DynamicImage::ImageRgb32F(straightened);
    let (w, h) = corrected.dimensions();
    if w == 0 || h == 0 {
        return Err("decoded image is empty".to_string());
    }
    let rgb8 = corrected.to_rgb8();
    let tmp: PathBuf = dst.with_extension("jpg.tmp");
    {
        let mut out = std::io::BufWriter::new(
            std::fs::File::create(&tmp).map_err(|e| format!("cannot create output: {e}"))?,
        );
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut out,
            quality.clamp(50, 100),
        );
        use image::ImageEncoder;
        encoder
            .write_image(&rgb8, w, h, image::ExtendedColorType::Rgb8)
            .map_err(|e| format!("JPEG encode failed: {e}"))?;
    }
    std::fs::rename(&tmp, dst).map_err(|e| format!("cannot finalize output: {e}"))?;
    Ok((w, h))
}

/// v2: decode, auto-correct with look, optional face retouch, write JPEG.
/// `retouch` 0 = off, 1 = teeth whiten + skin texture sharpen. Retouch
/// needs `ort_lib` (path to the onnxruntime shared library) and
/// `model_dir` (seeta_fd_frontal_v1.0.bin + face_mesh.onnx). When retouch
/// is off those may be null paths and are never read.
pub fn auto_correct_file_v2(
    src: &Path,
    dst: &Path,
    quality: u8,
    look: pipeline::Look,
    retouch: u8,
    ort_lib: Option<&Path>,
    model_dir: Option<&Path>,
) -> Result<(u32, u32), String> {
    let mut decoded = decode_to_dynamic(src)?;
    if let Some(md) = model_dir {
        decoded = retouch::auto_upright(decoded, md).0;
    }
    let corrected = pipeline::auto_correct(&decoded, look);
    let straightened = straighten::auto_straighten(&corrected.to_rgb32f());
    let corrected = DynamicImage::ImageRgb32F(straightened);
    let (w, h) = corrected.dimensions();
    if w == 0 || h == 0 {
        return Err("decoded image is empty".to_string());
    }
    let mut rgb8 = corrected.to_rgb8();
    if retouch != 0 {
        let ort_lib = ort_lib.ok_or_else(|| "retouch needs the onnxruntime path".to_string())?;
        let model_dir = model_dir.ok_or_else(|| "retouch needs the model folder".to_string())?;
        match retouch::retouch_image(&mut rgb8, ort_lib, model_dir) {
            Ok(_) => {}
            // Retouch failure must never block the normal export.
            Err(e) => eprintln!("retouch skipped: {e}"),
        }
    }
    let tmp: PathBuf = dst.with_extension("jpg.tmp");
    {
        let mut out = std::io::BufWriter::new(
            std::fs::File::create(&tmp).map_err(|e| format!("cannot create output: {e}"))?,
        );
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut out,
            quality.clamp(50, 100),
        );
        use image::ImageEncoder;
        encoder
            .write_image(&rgb8, w, h, image::ExtendedColorType::Rgb8)
            .map_err(|e| format!("JPEG encode failed: {e}"))?;
    }
    std::fs::rename(&tmp, dst).map_err(|e| format!("cannot finalize output: {e}"))?;
    Ok((w, h))
}

/// Small developed preview (for the retouch before/after view), returned
/// as a base64 JPEG. Same pipeline as the real export, downscaled.
pub fn preview_jpeg_base64(
    src: &Path,
    max_dim: u32,
    look: pipeline::Look,
    retouch: u8,
    ort_lib: Option<&Path>,
    model_dir: Option<&Path>,
) -> Result<String, String> {
    let mut decoded = decode_to_dynamic(src)?;
    if let Some(md) = model_dir {
        decoded = retouch::auto_upright(decoded, md).0;
    }
    let corrected = pipeline::auto_correct(&decoded, look);
    let straightened = straighten::auto_straighten(&corrected.to_rgb32f());
    let corrected = DynamicImage::ImageRgb32F(straightened);
    let max_dim = max_dim.clamp(256, 4096);
    let small = corrected.resize(max_dim, max_dim, image::imageops::FilterType::Triangle);
    let mut rgb8 = small.to_rgb8();
    if retouch != 0 {
        let ort_lib = ort_lib.ok_or_else(|| "retouch needs the onnxruntime path".to_string())?;
        let model_dir = model_dir.ok_or_else(|| "retouch needs the model folder".to_string())?;
        retouch::retouch_image(&mut rgb8, ort_lib, model_dir)?;
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85);
    use image::ImageEncoder;
    let (w, h) = rgb8.dimensions();
    encoder
        .write_image(&rgb8, w, h, image::ExtendedColorType::Rgb8)
        .map_err(|e| format!("JPEG encode failed: {e}"))?;
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(buf.into_inner()))
}

// ---------------------------------------------------------------------------
// C ABI used by the desktop app through framesense_raw.dll
// ---------------------------------------------------------------------------

static LAST_ERROR: Mutex<Option<CString>> = Mutex::new(None);

fn set_last_error(message: String) {
    if let Ok(mut slot) = LAST_ERROR.lock() {
        *slot = CString::new(message).ok();
    }
}

fn take_last_error() -> CString {
    match LAST_ERROR.lock() {
        Ok(mut slot) => slot.take().unwrap_or_else(|| CString::new("unknown error").unwrap()),
        Err(_) => CString::new("unknown error").unwrap(),
    }
}

fn read_c_path<'a>(ptr: *const c_char, what: &str) -> Result<PathBuf, String> {
    if ptr.is_null() {
        return Err(format!("{what} path is null"));
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| format!("{what} path is not valid UTF-8"))?;
    Ok(PathBuf::from(text))
}

/// Process one photo file. `dst` must end in .jpg; its parent must exist.
/// Returns 0 on success, or an error code (message via fs_raw_last_error):
///   1 = decode/develop failed   2 = output write failed   3 = bad arguments
#[no_mangle]
pub extern "C" fn fs_raw_auto_correct_file(
    src: *const c_char,
    dst: *const c_char,
    quality: u8,
) -> i32 {
    let run = || -> Result<(), String> {
        let src = read_c_path(src, "source")?;
        let dst = read_c_path(dst, "destination")?;
        if !src.is_file() {
            return Err(format!("source file not found: {}", src.display()));
        }
        match auto_correct_file(&src, &dst, quality) {
            Ok(_) => Ok(()),
            Err(e) if e.contains("cannot create output") || e.contains("cannot finalize") => {
                set_last_error(e);
                return Err("io".to_string());
            }
            Err(e) => Err(e),
        }
    };
    match run() {
        Ok(()) => 0,
        Err(e) if e == "io" => 2,
        Err(e) => {
            set_last_error(e);
            1
        }
    }
}

/// Process one photo with an explicit session look (0 natural, 1 warm,
/// 2 golden). Same return codes as fs_raw_auto_correct_file.
#[no_mangle]
pub extern "C" fn fs_raw_auto_correct_file_look(
    src: *const c_char,
    dst: *const c_char,
    quality: u8,
    look: u8,
) -> i32 {
    let run = || -> Result<(), String> {
        let src = read_c_path(src, "source")?;
        let dst = read_c_path(dst, "destination")?;
        if !src.is_file() {
            return Err(format!("source file not found: {}", src.display()));
        }
        match auto_correct_file_with_look(&src, &dst, quality, pipeline::Look::from_u8(look)) {
            Ok(_) => Ok(()),
            Err(e) if e.contains("cannot create output") || e.contains("cannot finalize") => {
                set_last_error(e);
                return Err("io".to_string());
            }
            Err(e) => Err(e),
        }
    };
    match run() {
        Ok(()) => 0,
        Err(e) if e == "io" => 2,
        Err(e) => {
            set_last_error(e);
            1
        }
    }
}

/// v2 entry: look + optional face retouch. Same return codes as
/// fs_raw_auto_correct_file. ort_lib/model_dir may be null when retouch=0.
#[no_mangle]
pub extern "C" fn fs_raw_auto_correct_file_v2(
    src: *const c_char,
    dst: *const c_char,
    quality: u8,
    look: u8,
    retouch: u8,
    ort_lib: *const c_char,
    model_dir: *const c_char,
) -> i32 {
    let run = || -> Result<(), String> {
        let src = read_c_path(src, "source")?;
        let dst = read_c_path(dst, "destination")?;
        if !src.is_file() {
            return Err(format!("source file not found: {}", src.display()));
        }
        let ort = if ort_lib.is_null() { None } else { Some(read_c_path(ort_lib, "onnxruntime")?) };
        let models = if model_dir.is_null() { None } else { Some(read_c_path(model_dir, "model folder")?) };
        match auto_correct_file_v2(
            &src,
            &dst,
            quality,
            pipeline::Look::from_u8(look),
            retouch,
            ort.as_deref(),
            models.as_deref(),
        ) {
            Ok(_) => Ok(()),
            Err(e) if e.contains("cannot create output") || e.contains("cannot finalize") => {
                set_last_error(e);
                return Err("io".to_string());
            }
            Err(e) => Err(e),
        }
    };
    match run() {
        Ok(()) => 0,
        Err(e) if e == "io" => 2,
        Err(e) => {
            set_last_error(e);
            1
        }
    }
}

/// True when the prominent faces in this (already corrected) photo have
/// closed or lowered eyes. Uses the open/closed-eye model on eye crops;
/// a face counts as closed only when BOTH eyes read closed with high
/// confidence, and any face at least half the width of the largest one counts.
pub fn photo_has_closed_eyes(path: &Path, ort_lib: &Path, model_dir: &Path) -> Result<bool, String> {
    let img = image::open(path).map_err(|e| format!("image open failed: {e}"))?;
    let img = if img.width().max(img.height()) > 1600 {
        img.resize(1600, 1600, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let faces = retouch::eye_states(&img.to_rgb8(), ort_lib, model_dir)?;
    let max_w = faces.iter().map(|f| f.0).fold(0.0f32, f32::max);
    Ok(faces
        .iter()
        .any(|(w, l, r)| *w >= 0.5 * max_w && *l < 0.15 && *r < 0.15))
}

/// C ABI: 1 = closed/lowered eyes found, 0 = fine or no faces, -1 = unknown
/// (error text via fs_raw_last_error).
#[no_mangle]
pub extern "C" fn fs_raw_eyes_closed(
    src: *const c_char,
    ort_lib: *const c_char,
    model_dir: *const c_char,
) -> i32 {
    let run = || -> Result<bool, String> {
        let src = read_c_path(src, "source")?;
        let ort = read_c_path(ort_lib, "onnxruntime")?;
        let models = read_c_path(model_dir, "model folder")?;
        photo_has_closed_eyes(&src, &ort, &models)
    };
    match run() {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(e) => {
            set_last_error(e);
            -1
        }
    }
}

/// Probe one photo and suggest a session tone. Returns a JSON string
/// {"look":N,"reason":"..."} (caller frees via fs_raw_free_string), or
/// null on error (fs_raw_last_error has the message).
#[no_mangle]
pub extern "C" fn fs_raw_probe_look(src: *const c_char) -> *mut c_char {
    let run = || -> Result<String, String> {
        let src = read_c_path(src, "source")?;
        if !src.is_file() {
            return Err(format!("source file not found: {}", src.display()));
        }
        let decoded = decode_to_dynamic(&src)?.to_rgb8();
        let small = if decoded.width() > 1024 {
            image::DynamicImage::ImageRgb8(decoded)
                .resize(1024, 1024, image::imageops::FilterType::Triangle)
                .to_rgb8()
        } else {
            decoded
        };
        let rgb32 = image::DynamicImage::ImageRgb8(small).to_rgb32f();
        // cheap people hint: warm skin-tone share, no face model needed
        let stats = pipeline::channel_stats(&rgb32);
        let has_people = stats.skin_hue_median.is_some();
        let (look, reason) = pipeline::suggest_look(&rgb32, has_people);
        let code = match look {
            pipeline::Look::Natural => 0,
            pipeline::Look::Warm => 1,
            pipeline::Look::Golden => 2,
            pipeline::Look::PeachGold => 3,
            pipeline::Look::AmberIndoor => 4,
            pipeline::Look::BrightAiry => 5,
            pipeline::Look::Cinematic => 6,
            pipeline::Look::Cool => 7,
            pipeline::Look::Punchy => 8,
        };
        Ok(format!("{{\"look\":{code},\"reason\":\"{reason}\"}}"))
    };
    match run() {
        Ok(j) => match CString::new(j) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(e) => {
            set_last_error(e);
            std::ptr::null_mut()
        }
    }
}

/// Base64 JPEG preview (developed with look, optional retouch), small.
/// Free with fs_raw_free_string. Null on failure (see fs_raw_last_error).
#[no_mangle]
pub extern "C" fn fs_raw_preview_jpeg(
    src: *const c_char,
    max_dim: u32,
    look: u8,
    retouch: u8,
    ort_lib: *const c_char,
    model_dir: *const c_char,
) -> *mut c_char {
    let run = || -> Result<String, String> {
        let src = read_c_path(src, "source")?;
        if !src.is_file() {
            return Err(format!("source file not found: {}", src.display()));
        }
        let ort = if ort_lib.is_null() { None } else { Some(read_c_path(ort_lib, "onnxruntime")?) };
        let models = if model_dir.is_null() { None } else { Some(read_c_path(model_dir, "model folder")?) };
        preview_jpeg_base64(
            &src,
            max_dim,
            pipeline::Look::from_u8(look),
            retouch,
            ort.as_deref(),
            models.as_deref(),
        )
    };
    match run() {
        Ok(b64) => match CString::new(b64) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(e) => {
            set_last_error(e);
            std::ptr::null_mut()
        }
    }
}

/// Last error message. Free with fs_raw_free_string. Never null.
#[no_mangle]
pub extern "C" fn fs_raw_last_error() -> *mut c_char {
    take_last_error().into_raw()
}

#[no_mangle]
pub extern "C" fn fs_raw_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = CString::from_raw(s);
        }
    }
}

#[no_mangle]
pub extern "C" fn fs_raw_engine_version() -> *mut c_char {
    CString::new(env!("CARGO_PKG_VERSION")).unwrap().into_raw()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "framesense-raw-test-{label}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn jpeg_input_produces_jpeg_output() {
        let dir = temp_dir("jpeg");
        let src = dir.join("photo.JPEG");
        let mut img = RgbImage::new(64, 48);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgb([(x * 4) as u8, (y * 5) as u8, 128]);
        }
        DynamicImage::ImageRgb8(img).save(&src).unwrap();

        let dst = dir.join("out").join("photo.jpg");
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        let (w, h) = auto_correct_file(&src, &dst, 90).unwrap();
        assert_eq!((w, h), (64, 48));
        let bytes = std::fs::read(&dst).unwrap();
        assert_eq!(&bytes[0..2], &[0xFF, 0xD8], "output must be a JPEG");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_source_is_an_error_not_a_panic() {
        let dir = temp_dir("missing");
        let result = auto_correct_file(&dir.join("nope.cr2"), &dir.join("out.jpg"), 90);
        assert!(result.is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ffi_roundtrip_reports_errors() {
        let dir = temp_dir("ffi");
        let src = CString::new(dir.join("nope.cr2").to_string_lossy().to_string()).unwrap();
        let dst = CString::new(dir.join("out.jpg").to_string_lossy().to_string()).unwrap();
        let code = fs_raw_auto_correct_file(src.as_ptr(), dst.as_ptr(), 90);
        assert_eq!(code, 3 - 3 + 1); // decode-class error
        let msg_ptr = fs_raw_last_error();
        assert!(!msg_ptr.is_null());
        fs_raw_free_string(msg_ptr);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn extension_sets_cover_common_raw() {
        for ext in ["cr2", "CR3", "Arw", "nef", "RAF", "dng", "orf", "rw2"] {
            assert!(is_raw_extension(ext), "{ext} must decode through rawler");
        }
        for ext in ["jpg", "jpeg", "png", "webp"] {
            assert!(!is_raw_extension(ext));
        }
    }
}
