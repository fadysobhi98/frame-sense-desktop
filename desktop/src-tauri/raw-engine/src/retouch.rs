//! Optional face retouch: teeth whitening, skin texture sharpening, skin
//! tone evening, small-blemish cleanup, subtle under-eye and eye-white
//! cleanup.
//!
//! Honest scope: face LANDMARKS come from a real AI model (Google MediaPipe
//! Face Mesh, Apache-2.0, ONNX conversion by yakhyo/mediapipe-face-mesh-onnx,
//! Apache-2.0) running locally on CPU. The retouch itself is deterministic
//! auto-correction driven by those landmarks: no generative fill, no skin
//! smoothing, no face reshaping. Blemish cleanup targets small red/dark
//! spots (a few pixels across) and deliberately skips plain dark marks such
//! as moles, which are part of a face's identity. Everything runs offline;
//! originals are never touched (the caller writes to a new file).

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
const LEFT_EYE_LOWER: [usize; 6] = [33, 145, 153, 154, 155, 133];
const RIGHT_EYE_LOWER: [usize; 7] = [263, 249, 390, 373, 374, 380, 362];
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
    min_size: u32,
    min_score: f64,
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
        detector.set_min_face_size(min_size);
        let image_data = rustface::ImageData::new(gray, w, h);
        Ok(detector
            .detect(&image_data)
            .into_iter()
            .filter(|f| f.score() >= min_score)
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
            let t_sat = ((0.65 - sat) / 0.40).clamp(0.0, 1.0);
            let t_val = ((mx - 0.06) / 0.14).clamp(0.0, 1.0);
            let wgt = m * t_sat * t_val;
            if wgt < 0.01 {
                continue;
            }
            let desat = 0.75 * wgt;
            let dr = r + (mx - r) * desat;
            let dg = g + (mx - g) * desat;
            let db = b + (mx - b) * desat;
            let lift = 0.50 * wgt;
            let lift_px = |v: f32| v + lift * (1.0 - v) * (1.0 - v);
            let nr = lift_px(dr).clamp(0.0, 1.0);
            let ng = lift_px(dg).clamp(0.0, 1.0);
            let nb = lift_px(db).clamp(0.0, 1.0);
            let blend = 0.95;
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
    // Band-pass texture boost: boost medium-scale detail (pores, 2-8px)
    // without touching pixel-scale grain. fine keeps pores, mid is the
    // local skin tone; their difference is texture without 1px noise, so a
    // grainy photo stays clean instead of turning to speckle.
    let fine = image::imageops::blur(&region_img, 0.8);
    let mid = image::imageops::blur(&region_img, 3.0);
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
            let fpx = fine.get_pixel(x - rx0, y - ry0);
            let mpx = mid.get_pixel(x - rx0, y - ry0);
            let strength = 0.9 * m_face[i];
            let mut np = [0u8; 3];
            for c in 0..3 {
                let band = fpx[c] as f32 - mpx[c] as f32;
                let v = p[c] as f32 + band * strength;
                np[c] = v.clamp(0.0, 255.0) as u8;
            }
            img.put_pixel(x, y, Rgb(np));
        }
    }

    retouch_skin_detail(img, rx0, ry0, rx1, ry1, side, &pt, &m_face, &m_le, &m_re, &m_lb, &m_rb, &m_mo);
}

/// Extended retouch on one face region, in YCbCr so chroma edits never
/// smear pores (luminance) and luminance edits never shift colour.
/// Steps: small-blemish cleanup, skin tone/redness evening, subtle
/// under-eye lift, subtle eye-white cleanup.
#[allow(clippy::too_many_arguments)]
fn retouch_skin_detail(
    img: &mut RgbImage,
    rx0: u32,
    ry0: u32,
    rx1: u32,
    ry1: u32,
    side: f32,
    pt: &dyn Fn(usize) -> (f32, f32),
    m_face: &[f32],
    m_le: &[f32],
    m_re: &[f32],
    m_lb: &[f32],
    m_rb: &[f32],
    m_mo: &[f32],
) {
    let (w, h) = (img.width(), img.height());
    let rw = (rx1 - rx0) as usize;
    let rh = (ry1 - ry0) as usize;
    let n = rw * rh;
    let (rwu, rhu) = (rw as u32, rh as u32);

    // region-local YCbCr buffers (Y 0..255, Cb/Cr 0..255 centered on 128)
    let mut yy = vec![0f32; n];
    let mut cb = vec![0f32; n];
    let mut cr = vec![0f32; n];
    // skin weight per region pixel (feathered oval, features cut out)
    let mut m_skin = vec![0f32; n];
    for ry in 0..rh {
        for rx in 0..rw {
            let li = ry * rw + rx;
            let gi = ((ry0 + ry as u32) * w + rx0 + rx as u32) as usize;
            let p = *img.get_pixel(rx0 + rx as u32, ry0 + ry as u32);
            let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
            yy[li] = 0.299 * r + 0.587 * g + 0.114 * b;
            cb[li] = 128.0 - 0.168736 * r - 0.331264 * g + 0.5 * b;
            cr[li] = 128.0 + 0.5 * r - 0.418688 * g - 0.081312 * b;
            let mut m = m_face[gi];
            if m_le[gi] > 0.0 || m_re[gi] > 0.0 || m_lb[gi] > 0.0 || m_rb[gi] > 0.0 || m_mo[gi] > 0.01 {
                m = 0.0;
            }
            if m > 0.0 && !is_skin(&p) {
                m = 0.0;
            }
            m_skin[li] = m;
        }
    }

    let blur = |buf: &[f32], rad: i32| blur_f(buf, rwu, rhu, rad, 0, 0, rwu, rhu);

    // --- small-blemish heal ---
    // DoG blob detection at spot scale (dark+red cores, or strong red),
    // then per-blob repair: low-frequency teleport fill from surrounding
    // skin + fine-texture transplant from a horizontal donor offset, so
    // healed areas keep real pores instead of going plastic-smooth.
    // Candidates need m_det (inside face oval, features cut) but no skin-
    // chroma gate: inflamed spots sit outside the normal skin chroma box.
    // Plain dark marks with no red component (moles) are mostly skipped by
    // the red requirement, but a red-toned mole can be healed too: Retouch
    // is opt-in and previewed, so this is acceptable and matches the
    // target tools' behaviour.
    let mut m_det = vec![0f32; n];
    for ry in 0..rh {
        for rx in 0..rw {
            let li = ry * rw + rx;
            let gi = ((ry0 + ry as u32) * w + rx0 + rx as u32) as usize;
            let mut m = m_face[gi];
            if m_le[gi] > 0.0 || m_re[gi] > 0.0 || m_lb[gi] > 0.0 || m_rb[gi] > 0.0 || m_mo[gi] > 0.01 {
                m = 0.0;
            }
            m_det[li] = m;
        }
    }
    let y_dog_lo = blur(&yy, 2);
    let y_dog_hi = blur(&yy, 7);
    let cr_dog_lo = blur(&cr, 2);
    let cr_dog_hi = blur(&cr, 7);
    let mut spot = vec![false; n];
    for i in 0..n {
        if m_det[i] < 0.4 {
            continue;
        }
        let dog_y = y_dog_lo[i] - y_dog_hi[i];
        let dog_cr = cr_dog_lo[i] - cr_dog_hi[i];
        if (dog_y < -6.0 && dog_cr > 1.0) || dog_cr > 4.0 {
            spot[i] = true;
        }
    }
    // connected components; keep spot-sized blobs only
    let max_area = ((side * side * 0.0009) as usize).clamp(8, 400);
    let mut comp = vec![false; n];
    let mut seen = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    for i in 0..n {
        if !spot[i] || seen[i] {
            continue;
        }
        stack.clear();
        stack.push(i);
        seen[i] = true;
        let mut members: Vec<usize> = Vec::new();
        while let Some(c) = stack.pop() {
            members.push(c);
            let (cx, cy) = (c % rw, c / rw);
            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (cx as i32 + dx, cy as i32 + dy);
                if nx < 0 || ny < 0 || nx >= rw as i32 || ny >= rh as i32 {
                    continue;
                }
                let ni = ny as usize * rw + nx as usize;
                if spot[ni] && !seen[ni] {
                    seen[ni] = true;
                    stack.push(ni);
                }
            }
        }
        if members.len() >= 4 && members.len() <= max_area {
            for &mi in &members {
                comp[mi] = true;
            }
        }
    }
    // dilate the fill set by 2px so the whole discoloured rim is healed
    let mut hole = vec![false; n];
    for i in 0..n {
        if !comp[i] {
            continue;
        }
        let (cx, cy) = (i % rw, i / rw);
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let (nx, ny) = (cx as i32 + dx, cy as i32 + dy);
                if nx >= 0 && ny >= 0 && nx < rw as i32 && ny < rh as i32 {
                    hole[ny as usize * rw + nx as usize] = true;
                }
            }
        }
    }
    let hole_count = hole.iter().filter(|&&v| v).count();
    // debug: dump the detection mask for tuning
    if std::env::var("FS_DEBUG_MASK").is_ok() {
        let mut dbg = image::GrayImage::new(rwu, rhu);
        for i in 0..n {
            let (x, y) = (i % rw, i / rw);
            let v = if hole[i] { 255u8 } else if spot[i] { 128 } else { (m_det[i] * 60.0) as u8 };
            dbg.put_pixel(x as u32, y as u32, image::Luma([v]));
        }
        let _ = dbg.save("/tmp/fs_debug_mask.png");
    }
    // 8% of the region can be genuine acne coverage; beyond that it's a
    // detection runaway on texture, so bail rather than heal half the face
    if hole_count > 0 && hole_count * 12 < n {
        // fine texture layer (pores) and low-frequency layer (tone)
        let y_low = blur(&yy, 3);
        // teleport fill: repeatedly blur+composite inside holes; converges
        // to a smooth fill fed by the surrounding skin
        let mut fy = y_low.clone();
        let mut fcb = cb.clone();
        let mut fcr = cr.clone();
        for _ in 0..14 {
            let by = blur(&fy, 3);
            let bcb = blur(&fcb, 3);
            let bcr = blur(&fcr, 3);
            for i in 0..n {
                if hole[i] {
                    fy[i] = by[i];
                    fcb[i] = bcb[i];
                    fcr[i] = bcr[i];
                }
            }
        }
        // texture donor: horizontal offset, prefer the side that is not
        // itself a hole; fall back to no texture (flat) when both are
        let shift = ((side * 0.045).max(12.0)) as i32;
        let hole_f: Vec<f32> = hole.iter().map(|&v| if v { 1.0f32 } else { 0.0 }).collect();
        let hf = blur(&hole_f, 2);
        for i in 0..n {
            let m = hf[i];
            if m < 0.02 {
                continue;
            }
            let (x, y) = (i % rw, i / rw);
            let donor = |dx: i32| -> Option<f32> {
                let nx = x as i32 + dx;
                if nx < 0 || nx >= rw as i32 {
                    return None;
                }
                let ni = y * rw + nx as usize;
                if hole[ni] {
                    None
                } else {
                    Some(yy[ni] - y_low[ni])
                }
            };
            // sample texture from the nearer cheek side first
            let tex = donor(-shift)
                .or_else(|| donor(shift))
                .unwrap_or(0.0);
            let healed_y = fy[i] + tex;
            yy[i] += (healed_y - yy[i]) * m;
            cb[i] += (fcb[i] - cb[i]) * m;
            cr[i] += (fcr[i] - cr[i]) * m;
        }
    }

    // --- skin tone / redness evening ---
    // Smooth chroma (never luminance, so pores stay) with an edge-aware
    // weight: pixels whose luminance matches their surround blend more.
    let rad_even = ((side * 0.02).max(2.0)) as i32;
    let cb_b = blur(&cb, rad_even);
    let cr_b = blur(&cr, rad_even);
    let y_b = blur(&yy, rad_even);
    // blend weight first, then blur the weight itself: a per-pixel weight
    // computed from noisy luminance would otherwise speckle the chroma
    let mut k_map = vec![0f32; n];
    for i in 0..n {
        let lum_sim = (1.0 - (yy[i] - y_b[i]).abs() / 14.0).clamp(0.0, 1.0);
        k_map[i] = 0.5 * m_skin[i] * lum_sim;
    }
    let k_map = blur(&k_map, 3);
    for i in 0..n {
        let k = k_map[i];
        if k < 0.01 {
            continue;
        }
        cb[i] += (cb_b[i] - cb[i]) * k;
        cr[i] += (cr_b[i] - cr[i]) * k;
    }

    // --- subtle under-eye lift ---
    // band under each lower eyelid, ~1 eye-height deep, well feathered
    let to_region = |i: usize| {
        let (x, y) = pt(i);
        (x - rx0 as f32, y - ry0 as f32)
    };
    for lower in [&LEFT_EYE_LOWER[..], &RIGHT_EYE_LOWER[..]] {
        let mut poly: Vec<(f32, f32)> = lower.iter().map(|&i| to_region(i)).collect();
        let y_top = poly.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let y_bot = poly.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        let depth = (y_bot - y_top).max(side * 0.02) * 1.1 + side * 0.02;
        for j in (0..poly.len()).rev() {
            poly.push((poly[j].0, poly[j].1 + depth));
        }
        let m_ue = blur(&poly_mask_f(rwu, rhu, &poly, 0, 0, rwu, rhu), ((side * 0.02).max(2.0)) as i32);
        for i in 0..n {
            let m = m_ue[i] * 0.8;
            if m < 0.02 {
                continue;
            }
            yy[i] += 0.07 * m * (255.0 - yy[i]);
            cb[i] += (128.0 - cb[i]) * 0.10 * m;
            cr[i] += (128.0 - cr[i]) * 0.14 * m;
        }
    }

    // --- subtle eye-white cleanup ---
    // eye interior pixels that are bright and desaturated (sclera, not iris):
    // pull down redness, tiny lift. Never touches the iris.
    let feather_eye = ((side * 0.008).max(1.0)) as i32;
    for eye in [&LEFT_EYE[..], &RIGHT_EYE[..]] {
        let poly: Vec<(f32, f32)> = eye.iter().map(|&i| to_region(i)).collect();
        let m_e = blur(&poly_mask_f(rwu, rhu, &poly, 0, 0, rwu, rhu), feather_eye);
        for i in 0..n {
            let m = m_e[i];
            if m < 0.05 {
                continue;
            }
            let (rx, ry) = (i % rw, i / rw);
            let p = *img.get_pixel(rx0 + rx as u32, ry0 + ry as u32);
            let mx = p[0].max(p[1]).max(p[2]) as f32;
            let mn = p[0].min(p[1]).min(p[2]) as f32;
            if mx < 115.0 || (mx - mn) / mx.max(1.0) > 0.32 {
                continue;
            }
            let k = m * 0.6;
            cr[i] += (128.0 - cr[i]) * 0.22 * k;
            cb[i] += (128.0 - cb[i]) * 0.10 * k;
            yy[i] += 0.05 * k * (255.0 - yy[i]);
        }
    }

    // --- single write-back for every pixel this pass touched ---
    let _ = h;
    for ry in 0..rh {
        for rx in 0..rw {
            let i = ry * rw + rx;
            let gi = ((ry0 + ry as u32) * w + rx0 + rx as u32) as usize;
            let p = *img.get_pixel(rx0 + rx as u32, ry0 + ry as u32);
            let y0 = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
            let cb0 = 128.0 - 0.168736 * p[0] as f32 - 0.331264 * p[1] as f32 + 0.5 * p[2] as f32;
            let cr0 = 128.0 + 0.5 * p[0] as f32 - 0.418688 * p[1] as f32 - 0.081312 * p[2] as f32;
            if (yy[i] - y0).abs() < 0.05 && (cb[i] - cb0).abs() < 0.05 && (cr[i] - cr0).abs() < 0.05 {
                continue;
            }
            let _ = gi;
            let r = yy[i] + 1.402 * (cr[i] - 128.0);
            let g = yy[i] - 0.344136 * (cb[i] - 128.0) - 0.714136 * (cr[i] - 128.0);
            let b = yy[i] + 1.772 * (cb[i] - 128.0);
            img.put_pixel(
                rx0 + rx as u32,
                ry0 + ry as u32,
                Rgb([r.clamp(0.0, 255.0) as u8, g.clamp(0.0, 255.0) as u8, b.clamp(0.0, 255.0) as u8]),
            );
        }
    }
}

/// Detect faces, run the landmark model, retouch each face in place.
/// `model_dir` must contain seeta_fd_frontal_v1.0.bin and face_mesh.onnx.
pub fn retouch_image(img: &mut RgbImage, ort_lib: &Path, model_dir: &Path) -> Result<usize, String> {
    let gray = image::DynamicImage::ImageRgb8(img.clone()).to_luma8();
    let (gw, gh) = gray.dimensions();
    let det_path: PathBuf = model_dir.join("seeta_fd_frontal_v1.0.bin");
    let faces = detect_faces(&det_path, gray.as_raw(), gw, gh, 32, 2.5)?;
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

/// Per detected face: (face width px, closed-probability of the left eye,
/// of the right eye) from the Open Model Zoo open-closed-eye-0001 model
/// (Apache-2.0), applied to eye crops located by the face-mesh landmarks.
pub fn eye_states(img: &RgbImage, ort_lib: &Path, model_dir: &Path) -> Result<Vec<(f32, f32, f32)>, String> {
    let gray = image::DynamicImage::ImageRgb8(img.clone()).to_luma8();
    let (gw, gh) = gray.dimensions();
    let det_path: PathBuf = model_dir.join("seeta_fd_frontal_v1.0.bin");
    let faces = detect_faces(&det_path, gray.as_raw(), gw, gh, 40, 3.0)?;
    let mut out = Vec::new();
    if faces.is_empty() {
        return Ok(out);
    }
    let env = ort_env(ort_lib)?;
    let mut mesh = ort::session::Session::builder(env)
        .map_err(|e| format!("landmark session failed: {e}"))?
        .with_intra_threads(2)
        .map_err(|e| format!("landmark session failed: {e}"))?
        .commit_from_file(&model_dir.join("face_mesh.onnx"))
        .map_err(|e| format!("landmark model failed to load: {e}"))?;
    let mut eye = ort::session::Session::builder(env)
        .map_err(|e| format!("eye session failed: {e}"))?
        .with_intra_threads(1)
        .map_err(|e| format!("eye session failed: {e}"))?
        .commit_from_file(&model_dir.join("open_closed_eye.onnx"))
        .map_err(|e| format!("eye model failed to load: {e}"))?;
    for f in &faces {
        let bb = f.bbox();
        let side = bb.width().max(bb.height()) as f32 * 1.5;
        let cx = bb.x() as f32 + bb.width() as f32 / 2.0;
        let cy = bb.y() as f32 + bb.height() as f32 / 2.0;
        let x0 = (cx - side / 2.0).max(0.0);
        let y0 = (cy - side / 2.0).max(0.0);
        let x1 = (cx + side / 2.0).min(img.width() as f32);
        let y1 = (cy + side / 2.0).min(img.height() as f32);
        if x1 <= x0 + 8.0 || y1 <= y0 + 8.0 {
            continue;
        }
        let (cw, ch) = (x1 - x0, y1 - y0);
        let crop = image::imageops::crop_imm(img, x0 as u32, y0 as u32, cw as u32, ch as u32).to_image();
        let face192 = image::imageops::resize(&crop, 192, 192, image::imageops::FilterType::Triangle);
        let mut input = ndarray::Array4::<f32>::zeros((1, 3, 192, 192));
        for y in 0..192usize {
            for x in 0..192usize {
                let p = face192.get_pixel(x as u32, y as u32);
                for c in 0..3 {
                    input[[0, c, y, x]] = p[c] as f32 / 255.0;
                }
            }
        }
        let outs = mesh
            .run(ort::inputs![ort::value::Tensor::from_array(input).map_err(|e| e.to_string())?])
            .map_err(|e| format!("landmark inference failed: {e}"))?;
        let (_s, lm) = outs[0].try_extract_tensor::<f32>().map_err(|e| format!("landmark output unreadable: {e}"))?;
        let lm: Vec<f32> = lm.to_vec();
        if lm.len() < 468 * 3 {
            continue;
        }
        if let Ok((_s, sc)) = outs[1].try_extract_tensor::<f32>() {
            if !sc.is_empty() && sc[0] < 0.5 {
                continue;
            }
        }
        drop(outs);
        let pt = |i: usize| -> (f32, f32) { (x0 + lm[i * 3] / 192.0 * cw, y0 + lm[i * 3 + 1] / 192.0 * ch) };
        let mut pc = [0.0f32; 2];
        let mut ok = true;
        for (k, (ia, ib)) in [(33usize, 133usize), (362usize, 263usize)].iter().enumerate() {
            let (ax, ay) = pt(*ia);
            let (bx, by) = pt(*ib);
            let ew = ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt();
            let (mx, my) = ((ax + bx) / 2.0, (ay + by) / 2.0);
            let half = (ew * 0.75).max(6.0);
            let ex0 = (mx - half).max(0.0);
            let ey0 = (my - half).max(0.0);
            let ex1 = (mx + half).min(img.width() as f32);
            let ey1 = (my + half).min(img.height() as f32);
            if ex1 <= ex0 + 4.0 || ey1 <= ey0 + 4.0 {
                ok = false;
                break;
            }
            let ec = image::imageops::crop_imm(img, ex0 as u32, ey0 as u32, (ex1 - ex0) as u32, (ey1 - ey0) as u32).to_image();
            let e32 = image::imageops::resize(&ec, 32, 32, image::imageops::FilterType::Triangle);
            let mut inp = ndarray::Array4::<f32>::zeros((1, 3, 32, 32));
            for y in 0..32usize {
                for x in 0..32usize {
                    let p = e32.get_pixel(x as u32, y as u32);
                    // model expects BGR, (v - 127) / 255
                    inp[[0, 0, y, x]] = (p[2] as f32 - 127.0) / 255.0;
                    inp[[0, 1, y, x]] = (p[1] as f32 - 127.0) / 255.0;
                    inp[[0, 2, y, x]] = (p[0] as f32 - 127.0) / 255.0;
                }
            }
            let o = eye
                .run(ort::inputs![ort::value::Tensor::from_array(inp).map_err(|e| e.to_string())?])
                .map_err(|e| format!("eye inference failed: {e}"))?;
            let (_s, v) = o[0].try_extract_tensor::<f32>().map_err(|e| format!("eye output unreadable: {e}"))?;
            if v.len() < 2 {
                ok = false;
                break;
            }
            pc[k] = v[1]; // [open, closed]
        }
        if ok {
            out.push((bb.width() as f32, pc[0], pc[1]));
        }
    }
    Ok(out)
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

/// Content-based "make it upright": when the photo has no detectable face
/// as-is but does when turned 90/180/270 degrees, turn it. Used for files
/// whose camera orientation tag is missing or wrong. Faces are found with
/// the local SeetaFace detector (a model, not AI generation). Photos with no
/// detectable faces are left exactly as decoded. Returns (image, degrees).
pub fn auto_upright(img: image::DynamicImage, model_dir: &Path) -> (image::DynamicImage, u32) {
    let det_path: PathBuf = model_dir.join("seeta_fd_frontal_v1.0.bin");
    if !det_path.is_file() {
        return (img, 0);
    }
    let small = img.resize(720, 720, image::imageops::FilterType::Triangle);
    let score = |d: &image::DynamicImage| -> f64 {
        let g = d.to_luma8();
        let (w, h) = g.dimensions();
        match detect_faces(&det_path, g.as_raw(), w, h, 20, 3.0) {
            Ok(f) => f.iter().map(|x| x.score()).sum(),
            Err(_) => 0.0,
        }
    };
    let s0 = score(&small);
    if s0 >= 3.0 {
        return (img, 0);
    }
    let cands = [
        (90u32, small.rotate90()),
        (180u32, small.rotate180()),
        (270u32, small.rotate270()),
    ];
    let mut best = (0u32, 0.0f64);
    for (deg, d) in cands.iter() {
        let sc = score(d);
        if sc > best.1 {
            best = (*deg, sc);
        }
    }
    if best.1 >= 4.0 && best.1 > s0 * 2.0 {
        let out = match best.0 {
            90 => img.rotate90(),
            180 => img.rotate180(),
            _ => img.rotate270(),
        };
        return (out, best.0);
    }
    (img, 0)
}
