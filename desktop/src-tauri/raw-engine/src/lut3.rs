//! Optional "Auto AI look": Image-Adaptive 3D LUT (Zeng et al., Apache-2.0
//! code). A small neural network looks at each photo and blends three
//! learned colour tables; the blended table is then applied to the photo.
//! The pretrained weights were trained on the MIT-Adobe FiveK dataset,
//! whose licence is research-only: personal use only, never redistribute.
use image::Rgb32FImage;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const DIM: usize = 33;
const LUT_FLOATS: usize = 3 * DIM * DIM * DIM;

/// (enabled, strength 0..1, onnxruntime library, model folder)
static CONFIG: Mutex<Option<(f32, PathBuf, PathBuf)>> = Mutex::new(None);

pub fn configure(enabled: bool, strength: f32, ort_lib: PathBuf, model_dir: PathBuf) {
    if let Ok(mut c) = CONFIG.lock() {
        *c = if enabled && strength > 0.0 { Some((strength.clamp(0.0, 1.0), ort_lib, model_dir)) } else { None };
    }
}

fn read_luts(path: &Path) -> Result<Vec<f32>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("3D LUT table missing: {e}"))?;
    if bytes.len() != 3 * LUT_FLOATS * 4 {
        return Err("3D LUT table has the wrong size".to_string());
    }
    Ok(bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

/// Blend weights from the small classifier network (256x256 thumbnail).
fn weights(rgb: &Rgb32FImage, ort_lib: &Path, model_dir: &Path) -> Result<[f32; 3], String> {
    let env = crate::retouch::ort_env(ort_lib)?;
    let mut session = ort::session::Session::builder(env)
        .map_err(|e| e.to_string())?
        .with_intra_threads(2)
        .map_err(|e| e.to_string())?
        .commit_from_file(model_dir.join("lut3_classifier.onnx"))
        .map_err(|e| format!("3D LUT model failed to load: {e}"))?;
    let small = image::imageops::resize(rgb, 256, 256, image::imageops::FilterType::Triangle);
    let mut input = ndarray::Array4::<f32>::zeros((1, 3, 256, 256));
    for y in 0..256usize {
        for x in 0..256usize {
            let p = small.get_pixel(x as u32, y as u32);
            for c in 0..3 {
                input[[0, c, y, x]] = p[c].clamp(0.0, 1.0);
            }
        }
    }
    let outs = session
        .run(ort::inputs![ort::value::Tensor::from_array(input).map_err(|e| e.to_string())?])
        .map_err(|e| format!("3D LUT inference failed: {e}"))?;
    let (_s, w) = outs[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
    if w.len() < 3 {
        return Err("3D LUT model output unreadable".to_string());
    }
    Ok([w[0], w[1], w[2]])
}

/// Apply in place with the configured strength (0 = off). Returns Ok(false)
/// when the feature is off.
pub fn apply(rgb: &mut Rgb32FImage) -> Result<bool, String> {
    let cfg = CONFIG.lock().map_err(|_| "lut lock".to_string())?.clone();
    let Some((strength, ort_lib, model_dir)) = cfg else { return Ok(false) };
    let luts = read_luts(&model_dir.join("lut3_luts.bin"))?;
    let w = weights(rgb, &ort_lib, &model_dir)?;
    let mut lut = vec![0f32; LUT_FLOATS];
    for i in 0..LUT_FLOATS {
        lut[i] = w[0] * luts[i] + w[1] * luts[LUT_FLOATS + i] + w[2] * luts[2 * LUT_FLOATS + i];
    }
    let d = (DIM - 1) as f32;
    let plane = DIM * DIM * DIM;
    for px in rgb.pixels_mut() {
        let [r, g, b] = px.0;
        let (pr, pg, pb) = (r.clamp(0.0, 1.0) * d, g.clamp(0.0, 1.0) * d, b.clamp(0.0, 1.0) * d);
        let (r0, g0, b0) = ((pr.floor() as usize).min(DIM - 2), (pg.floor() as usize).min(DIM - 2), (pb.floor() as usize).min(DIM - 2));
        let (fr, fg, fb) = (pr - r0 as f32, pg - g0 as f32, pb - b0 as f32);
        let mut out = [0f32; 3];
        for (c, o) in out.iter_mut().enumerate() {
            let base = c * plane;
            let at = |dr: usize, dg: usize, db: usize| lut[base + (b0 + db) * DIM * DIM + (g0 + dg) * DIM + (r0 + dr)];
            let c00 = at(0, 0, 0) * (1.0 - fr) + at(1, 0, 0) * fr;
            let c10 = at(0, 1, 0) * (1.0 - fr) + at(1, 1, 0) * fr;
            let c01 = at(0, 0, 1) * (1.0 - fr) + at(1, 0, 1) * fr;
            let c11 = at(0, 1, 1) * (1.0 - fr) + at(1, 1, 1) * fr;
            let c0 = c00 * (1.0 - fg) + c10 * fg;
            let c1 = c01 * (1.0 - fg) + c11 * fg;
            *o = c0 * (1.0 - fb) + c1 * fb;
        }
        let src = [r, g, b];
        for c in 0..3 {
            px.0[c] = (src[c] * (1.0 - strength) + out[c].clamp(0.0, 1.0) * strength).clamp(0.0, 1.0);
        }
    }
    Ok(true)
}
