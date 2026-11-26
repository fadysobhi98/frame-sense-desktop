//! Optional face retouch: teeth whitening + skin texture sharpening.
//!
//! Honest scope: face LANDMARKS come from a real AI model (Google MediaPipe
//! Face Mesh, Apache-2.0, ONNX conversion by yakhyo/mediapipe-face-mesh-onnx,
//! Apache-2.0) running locally on CPU. The retouch itself is deterministic
//! auto-correction driven by those landmarks: no generative fill, no skin
//! smoothing, no face reshaping. Everything runs offline; originals are
//! never touched (the caller writes to a new file).

use image::{Rgb, RgbImage};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// MediaPipe Face Mesh 468-point index groups (canonical topology).
const FACE_OVAL: [usize; 36] = [10,338,297,332,284,251,389,356,454,323,361,288,397,365,379,378,400,377,152,148,176,149,150,136,172,58,132,93,234,127,162,21,54,103,67,109];
const LEFT_EYE: [usize; 16] = [33,7,163,144,145,153,154,155,133,173,157,158,159,160,161,246];
const RIGHT_EYE: [usize; 16] = [362,382,381,380,374,373,390,249,263,466,388,387,386,385,384,398];
const LEFT_BROW: [usize; 10] = [70,63,105,66,107,55,65,52,53,46];
const RIGHT_BROW: [usize; 10] = [336,296,334,293,300,283,295,282,283,336];
const LIPS_OUTER: [usize; 20] = [61,146,91,181,84,17,314,405,321,375,291,409,270,269,267,0,37,39,40,185];
const LIPS_INNER: [usize; 20] = [78,191,80,81,82,13,312,311,310,415,308,324,318,402,317,14,87,178,88,95];

thread_local! {
    static DETECTOR: RefCell<Option<Box<dyn rustface::Detector>>> = RefCell::new(None);
}

/// Detect faces with a per-thread cached SeetaFace detector (same model the
/// sorter uses). Faces below score 3.0 are dropped (same bar as classify.rs).
fn detect_faces(
    model: &Path,
    gray: &[u8],
    w: u32,
    h: u32,
) -> Result<Vec<rustface::FaceInfo>, String> {
    let model_str = model.to_string_lossy().into_owned();
    DETECTOR.with(|det| {
        let mut slot = det.borrow_mut();
        if slot.is_none() {
            let d = rustface::create_detector(&model_str)
                .map_err(|e| format!("face detector init failed: {e}"))?;
            *slot = Some(d);
        }
        let detector = slot.as_mut().unwrap();
        detector.set_min_face_size(48);
        let image_data = rustface::ImageData::new(gray, w, h);
        Ok(detector
            .detect(&image_data)
            .into_iter()
            .filter(|f| f.score() >= 3.0)
            .collect())
    })
}

static ORT_ENV: OnceLock<Result<ort::environment::Environment, String>> = OnceLock::new();
static ORT_INIT_MSG: Mutex<()> = Mutex::new(());

fn ort_env(ort_lib: &Path) -> Result<&'static ort::environment::Environment, String> {
    // Serialize the (one-time) init so two threads never race it.
    let _guard = ORT_INIT_MSG.lock().map_err(|_| "ort init lock".to_string())?;
    let res = ORT_ENV.get_or_init(|| {
        ort::init_from(ort_lib.to_string_lossy().into_owned())
            .map_err(|e| format!("onnxruntime not found: {e}"))?
            .build()
            .map_err(|e| format!("onnxruntime init failed: {e}"))
    });
    res.as_ref().map_err(|e| e.clone())
}

fn poly_mask_f(w: u32, h: u32, poly: &[(f32, f32)], x0: u32, y0: u32, x1: u32, y1: u32) -> Vec<f32> {
    let mut m = vec![0f32; (w * h) as usize];
    for y in y0..y1 {
        for x in x0..x1 {
            let mut inside = false;
            let n = poly.len();
            let mut j = n - 1;
            for i in 0..n {
                let (xi, yi) = poly[i];
                let (xj, yj) = poly[j];
                if ((yi > y as f32) != (yj > y as f32))
                    && ((x as f32) < (xj - xi) * (y as f32 - yi) / (yj - yi) + xi)
                {
                    inside = !inside;
                }
                j = i;
            }
            if inside {
                m[(y * w + x) as usize] = 1.0;
            }
        }
    }
    m
}

/// Two iterations of two-pass box blur: soft, feathered mask edges.
fn blur_f(m: &[f32], w: u32, h: u32, radius: i32, x0: u32, y0: u32, x1: u32, y1: u32) -> Vec<f32> {
    let (w, h) = (w as i32, h as i32);
    let (x0, y0, x1, y1) = (x0 as i32, y0 as i32, x1 as i32, y1 as i32);
    let mut cur = m.to_vec();
    for _ in 0..2 {
        let mut tmp = vec![0f32; (w * h) as usize];
        for y in y0..y1 {
            let mut acc = 0f32;
            for x in (x0 - radius)..=(x0 + radius) {
                acc += cur[(y * w + x.clamp(0, w - 1)) as usize];
            }
            for x in x0..x1 {
                tmp[(y * w + x) as usize] = acc / ((2 * radius + 1) as f32);
                let xsub = (x - radius).clamp(0, w - 1);
                let xadd = (x + radius + 1).clamp(0, w - 1);
                acc += cur[(y * w + xadd) as usize] - cur[(y * w + xsub) as usize];
            }
        }
        let mut out = vec![0f32; (w * h) as usize];
        for x in x0..x1 {
            let mut acc = 0f32;
            for y in (y0 - radius)..=(y0 + radius) {
                acc += tmp[(y.clamp(0, h - 1) * w + x) as usize];
            }
            for y in y0..y1 {
                out[(y * w + x) as usize] = acc / ((2 * radius + 1) as f32);
                let ysub = (y - radius).clamp(0, h - 1);
                let yadd = (y + radius + 1).clamp(0, h - 1);
                acc += tmp[(yadd * w + x) as usize] - tmp[(ysub * w + x) as usize];
            }
        }
        cur = out;
    }
    cur
}

fn is_skin(p: &Rgb<u8>) -> bool {
    let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
    let cb = 128 + ((-43 * r - 85 * g + 128 * b) >> 8);
    let cr = 128 + ((128 * r - 107 * g - 21 * b) >> 8);
    cb >= 77 && cb <= 135 && cr >= 125 && cr <= 185
}

/// Retouch one face region in place: whiten teeth, sharpen skin texture.
fn retouch_face(
    img: &mut RgbImage,
    box_x: f32,
    box_y: f32,
    box_w: f32,
    box_h: f32,
    lm: &[f32],
) {
    let (w, h) = (img.width(), img.height());
    // landmarks are in 0..192 of a crop that had 1.5x margin around the box
    let side = box_w.max(box_h) * 1.5;
    let cx = box_x + box_w / 2.0;
    let cy = box_y + box_h / 2.0;
    let cx0 = (cx - side / 2.0).max(0.0);
    let cy0 = (cy - side / 2.0).max(0.0);
    let cx1 = (cx + side / 2.0).min(w as f32);
    let cy1 = (cy + side / 2.0).min(h as f32);
    let cw = cx1 - cx0;
    let ch = cy1 - cy0;
    let pt = |i: usize| -> (f32, f32) { (cx0 + lm[i * 3] / 192.0 * cw, cy0 + lm[i * 3 + 1] / 192.0 * ch) };

    // work region: the face crop bounds
    let rx0 = (cx0 as u32).saturating_sub(4);
    let ry0 = (cy0 as u32).saturating_sub(4);
    let rx1 = ((cx1 as u32) + 4).min(w);
    let ry1 = ((cy1 as u32) + 4).min(h);

    let to_poly = |idx: &[usize]| idx.iter().map(|&i| pt(i)).collect::<Vec<_>>();
    let feather = ((side * 0.012).max(1.5)) as i32;

    // --- teeth whiten (soft mask, soft-shoulder lift, mild desaturation) ---
    let inner = to_poly(&LIPS_INNER);
    let region = blur_f(&poly_mask_f(w, h, &inner, rx0, ry0, rx1, ry1), w, h, feather, rx0, ry0, rx1, ry1);
    for y in ry0..ry1 {
        for x in rx0..rx1 {
            let i = (y * w + x) as usize;
            let m = region[i];
            if m < 0.01 {
                continue;
            }
            let p = *img.get_pixel(x, y);
            let (r, g, b) = (p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0);
            let mx = r.max(g).max(b);
            let mn = r.min(g).min(b);
            let sat = if mx > 1e-5 { (mx - mn) / mx } else { 0.0 };
            let t_sat = ((0.55 - sat) / 0.40).clamp(0.0, 1.0);
            let t_val = ((mx - 0.12) / 0.18).clamp(0.0, 1.0);
            let wgt = m * t_sat * t_val;
            if wgt < 0.01 {
                continue;
            }
            let desat = 0.55 * wgt;
            let dr = r + (mx - r) * desat;
            let dg = g + (mx - g) * desat;
            let db = b + (mx - b) * desat;
            let lift = 0.30 * wgt;
            let lift_px = |v: f32| v + lift * (1.0 - v) * (1.0 - v);
            let nr = lift_px(dr).clamp(0.0, 1.0);
            let ng = lift_px(dg).clamp(0.0, 1.0);
            let nb = lift_px(db).clamp(0.0, 1.0);
            let blend = 0.85;
            img.put_pixel(
                x,
                y,
                Rgb([
                    ((r * (1.0 - blend) + nr * blend) * 255.0) as u8,
                    ((g * (1.0 - blend) + ng * blend) * 255.0) as u8,
                    ((b * (1.0 - blend) + nb * blend) * 255.0) as u8,
                ]),
            );
        }
    }

    // --- skin texture sharpen (unsharp on skin only, never smoothing) ---
    let oval = to_poly(&FACE_OVAL);
    let m_face = blur_f(&poly_mask_f(w, h, &oval, rx0, ry0, rx1, ry1), w, h, feather, rx0, ry0, rx1, ry1);
    let m_le = poly_mask_f(w, h, &to_poly(&LEFT_EYE), rx0, ry0, rx1, ry1);
    let m_re = poly_mask_f(w, h, &to_poly(&RIGHT_EYE), rx0, ry0, rx1, ry1);
    let m_lb = poly_mask_f(w, h, &to_poly(&LEFT_BROW), rx0, ry0, rx1, ry1);
    let m_rb = poly_mask_f(w, h, &to_poly(&RIGHT_BROW), rx0, ry0, rx1, ry1);
    let m_mo = blur_f(&poly_mask_f(w, h, &to_poly(&LIPS_OUTER), rx0, ry0, rx1, ry1), w, h, feather, rx0, ry0, rx1, ry1);
    // blurred copy of just the face region for the unsharp base
    let rw = rx1 - rx0;
    let rh = ry1 - ry0;
    let region_img = image::imageops::crop_imm(img, rx0, ry0, rw, rh).to_image();
    let blurred = image::imageops::blur(&region_img, 2.0);
    for y in ry0..ry1 {
        for x in rx0..rx1 {
            let i = (y * w + x) as usize;
            if m_face[i] < 0.5 || m_le[i] > 0.0 || m_re[i] > 0.0 || m_lb[i] > 0.0 || m_rb[i] > 0.0 || m_mo[i] > 0.01 {
                continue;
            }
            let p = *img.get_pixel(x, y);
            if !is_skin(&p) {
                continue;
            }
            let b = blurred.get_pixel(x - rx0, y - ry0);
            let strength = 0.6 * m_face[i];
            let mut np = [0u8; 3];
            for c in 0..3 {
                let v = p[c] as f32 + (p[c] as f32 - b[c] as f32) * strength;
                np[c] = v.clamp(0.0, 255.0) as u8;
            }
            img.put_pixel(x, y, Rgb(np));
        }
    }
}

/// Detect faces, run the landmark model, retouch each face in place.
/// `model_dir` must contain seeta_fd_frontal_v1.0.bin and face_mesh.onnx.
pub fn retouch_image(img: &mut RgbImage, ort_lib: &Path, model_dir: &Path) -> Result<usize, String> {
    let gray = image::DynamicImage::ImageRgb8(img.clone()).to_luma8();
    let (gw, gh) = gray.dimensions();
    let det_path: PathBuf = model_dir.join("seeta_fd_frontal_v1.0.bin");
    let faces = detect_faces(&det_path, gray.as_raw(), gw, gh)?;
    if faces.is_empty() {
        return Ok(0);
    }

    let env = ort_env(ort_lib)?;
    let model_path = model_dir.join("face_mesh.onnx");
    let mut session = ort::session::Session::builder(env)
        .map_err(|e| format!("landmark session failed: {e}"))?
        .with_intra_threads(4)
        .map_err(|e| format!("landmark session failed: {e}"))?
        .commit_from_file(&model_path)
        .map_err(|e| format!("landmark model failed to load: {e}"))?;

    let mut done = 0usize;
    for f in &faces {
        let bb = f.bbox();
        let side = bb.width().max(bb.height()) as f32 * 1.5;
        let cx = bb.x() as f32 + bb.width() as f32 / 2.0;
        let cy = bb.y() as f32 + bb.height() as f32 / 2.0;
        let x0 = (cx - side / 2.0).max(0.0) as u32;
        let y0 = (cy - side / 2.0).max(0.0) as u32;
        let x1 = ((cx + side / 2.0) as u32).min(img.width());
        let y1 = ((cy + side / 2.0) as u32).min(img.height());
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let crop = image::imageops::crop_imm(img, x0, y0, x1 - x0, y1 - y0).to_image();
        let face192 = image::imageops::resize(&crop, 192, 192, image::imageops::FilterType::Triangle);
        let mut input = ndarray::Array4::<f32>::zeros((1, 3, 192, 192));
        for y in 0..192usize {
            for x in 0..192usize {
                let p = face192.get_pixel(x as u32, y as u32);
                input[[0, 0, y, x]] = p[0] as f32 / 255.0;
                input[[0, 1, y, x]] = p[1] as f32 / 255.0;
                input[[0, 2, y, x]] = p[2] as f32 / 255.0;
            }
        }
        let outs = session
            .run(ort::inputs![ort::value::Tensor::from_array(input).map_err(|e| e.to_string())?])
            .map_err(|e| format!("landmark inference failed: {e}"))?;
        let (_shape, lm) = outs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("landmark output unreadable: {e}"))?;
        let lm: Vec<f32> = lm.to_vec();
        if lm.len() < 468 * 3 {
            continue;
        }
        // confidence from the model's second output; skip weak detections
        if let Ok((_s, sc)) = outs[1].try_extract_tensor::<f32>() {
            if !sc.is_empty() && sc[0] < 0.5 {
                continue;
            }
        }
        retouch_face(img, bb.x() as f32, bb.y() as f32, bb.width() as f32, bb.height() as f32, &lm);
        done += 1;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_mask_basic() {
        let poly = vec![(2.0, 2.0), (6.0, 2.0), (6.0, 6.0), (2.0, 6.0)];
        let m = poly_mask_f(9, 9, &poly, 0, 0, 9, 9);
        assert_eq!(m[(4 * 9 + 4) as usize], 1.0);
        assert_eq!(m[(1 * 9 + 1) as usize], 0.0);
        // blur spreads mass outward but center stays strongest
        let b = blur_f(&m, 9, 9, 2, 0, 0, 9, 9);
        assert!(b[(4 * 9 + 4) as usize] > b[(0) as usize]);
    }

    #[test]
    fn skin_gate() {
        assert!(is_skin(&Rgb([200u8, 150, 130])));
        assert!(!is_skin(&Rgb([30u8, 200, 60])));
        assert!(!is_skin(&Rgb([200u8, 220, 245])));
    }
}
