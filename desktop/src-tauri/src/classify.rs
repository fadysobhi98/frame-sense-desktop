//! Photo sorting for Frame Sense (v0.4.0).
//!
//! After a photo is corrected, it is sorted into a subfolder of the output
//! folder:
//!   - "Rejected"  : clearly blurry (heuristic sharpness score)
//!   - "Group"     : 3 or more faces (real local face-detection model)
//!   - "Couple"    : exactly 2 faces
//!   - "Regular"   : everything else
//!
//! Honest labelling: the blur check is a heuristic (edge-strength score with
//! a fixed threshold), the face count uses a real trained face-detection
//! model (SeetaFace frontal detector, BSD-2-Clause, bundled, runs fully
//! offline on CPU). Sorting only ever places the corrected COPY; originals
//! are never moved, deleted, or modified, and Rejected is review-only.

use image::{DynamicImage, GrayImage};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Once;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
pub enum SortClass {
    Regular,
    Couple,
    Group,
    Rejected,
}

impl SortClass {
    pub fn subfolder(self) -> &'static str {
        match self {
            SortClass::Regular => "Regular",
            SortClass::Couple => "Couple",
            SortClass::Group => "Group",
            SortClass::Rejected => "Rejected",
        }
    }
}

#[derive(Clone, Copy, Default, Debug, serde::Serialize)]
pub struct SortCounts {
    pub regular: usize,
    pub couple: usize,
    pub group: usize,
    pub rejected: usize,
}

impl SortCounts {
    pub fn add(&mut self, class: SortClass) {
        match class {
            SortClass::Regular => self.regular += 1,
            SortClass::Couple => self.couple += 1,
            SortClass::Group => self.group += 1,
            SortClass::Rejected => self.rejected += 1,
        }
    }
}

/// Sharpness threshold: ratio of Laplacian variance to overall luminance
/// variance on a normalized small grayscale image. Normalizing by total
/// variance makes the score comparable across photos with different
/// texture and brightness; blur destroys high frequencies while keeping
/// the low-frequency variance, so the ratio drops sharply for defocused
/// shots. Below this the photo counts as clearly blurry.
/// Calibrated on real and synthetic samples; conservative on purpose so
/// mildly soft photos stay out of Rejected. Heuristic, not exact: borderline
/// photos can land on either side, which is why Rejected is review-only.
pub const REJECT_SHARPNESS: f64 = 0.03;

/// Minimum face-detector score to count a detection as a face.
const FACE_SCORE_MIN: f64 = 2.0;
/// Long-side cap for the image fed to the face detector (detection speed).
const FACE_DETECT_MAX_DIM: u32 = 1600;

const MODEL_BYTES: &[u8] = include_bytes!("../models/seeta_fd_frontal_v1.0.bin");

thread_local! {
    static DETECTOR: RefCell<Option<Box<dyn rustface::Detector>>> = RefCell::new(None);
}

static MODEL_INIT: Once = Once::new();

fn model_path() -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join("framesense_seeta_fd_frontal_v1.0.bin");
    let need_write = match std::fs::metadata(&path) {
        Ok(m) => m.len() as usize != MODEL_BYTES.len(),
        Err(_) => true,
    };
    if need_write {
        std::fs::write(&path, MODEL_BYTES)
            .map_err(|e| format!("Could not stage the face model: {e}"))?;
    }
    Ok(path)
}

fn load_model_once() {
    MODEL_INIT.call_once(|| {
        let _ = model_path();
    });
}

/// Count faces with the bundled SeetaFace detector. Any failure (model
/// unreadable, detector error) yields 0 so sorting degrades to Regular
/// rather than failing the export.
pub fn count_faces(img: &DynamicImage) -> usize {
    load_model_once();
    let path = match model_path() {
        Ok(p) => p,
        Err(_) => return 0,
    };
    let scaled = if img.width().max(img.height()) > FACE_DETECT_MAX_DIM {
        img.resize(
            FACE_DETECT_MAX_DIM,
            FACE_DETECT_MAX_DIM,
            image::imageops::FilterType::Triangle,
        )
    } else {
        img.clone()
    };
    let gray = scaled.to_luma8();
    let (w, h) = gray.dimensions();
    if w == 0 || h == 0 {
        return 0;
    }
    DETECTOR.with(|det| {
        let mut slot = det.borrow_mut();
        if slot.is_none() {
            match rustface::create_detector(path.to_str().unwrap_or("")) {
                Ok(d) => *slot = Some(d),
                Err(_) => return 0,
            }
        }
        let detector = slot.as_mut().unwrap();
        let image_data = rustface::ImageData::new(gray.as_raw(), w, h);
        detector
            .detect(&image_data)
            .iter()
            .filter(|f| f.score() >= FACE_SCORE_MIN)
            .count()
    })
}

/// Sharpness score: variance of the Laplacian divided by the variance of
/// the image itself, on a small grayscale copy. Higher = sharper. The
/// normalization makes it comparable across photos with different content.
pub fn sharpness_score(img: &DynamicImage) -> f64 {
    let small = img.resize(512, 512, image::imageops::FilterType::Triangle);
    let gray: GrayImage = small.to_luma8();
    let (w, h) = (gray.width() as i64, gray.height() as i64);
    if w < 3 || h < 3 {
        return 0.0;
    }
    let px = gray.as_raw();
    let stride = gray.width() as i64;
    let mut sum = 0f64;
    let mut sum_sq = 0f64;
    let mut n = 0f64;
    let mut lsum = 0f64;
    let mut lsum_sq = 0f64;
    let mut ln = 0f64;
    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let i = (y * stride + x) as usize;
            let v = px[i] as f64;
            lsum += v;
            lsum_sq += v * v;
            ln += 1.0;
            let lap = 4.0 * v
                - px[(i as i64 - 1) as usize] as f64
                - px[(i as i64 + 1) as usize] as f64
                - px[(i as i64 - stride) as usize] as f64
                - px[(i as i64 + stride) as usize] as f64;
            sum += lap;
            sum_sq += lap * lap;
            n += 1.0;
        }
    }
    if n == 0.0 || ln == 0.0 {
        return 0.0;
    }
    let mean = sum / n;
    let lap_var = (sum_sq / n) - mean * mean;
    let lmean = lsum / ln;
    let img_var = (lsum_sq / ln) - lmean * lmean;
    // +25 keeps very dark or nearly flat photos from exploding the ratio.
    lap_var / (img_var + 25.0)
}

/// Classify one corrected output photo. Blur is checked first: a clearly
/// blurry photo is Rejected regardless of who is in it.
pub fn classify_output(path: &Path) -> SortClass {
    let img = match image::open(path) {
        Ok(i) => i,
        Err(_) => return SortClass::Regular,
    };
    if sharpness_score(&img) < REJECT_SHARPNESS {
        return SortClass::Rejected;
    }
    match count_faces(&img) {
        2 => SortClass::Couple,
        n if n >= 3 => SortClass::Group,
        _ => SortClass::Regular,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn gradient_image() -> DynamicImage {
        // High-frequency checkerboard: very sharp.
        let mut img = RgbImage::new(256, 256);
        for y in 0..256u32 {
            for x in 0..256u32 {
                let v = if (x + y) % 2 == 0 { 255 } else { 0 };
                img.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        DynamicImage::ImageRgb8(img)
    }

    fn flat_image() -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(256, 256, Rgb([128, 128, 128])))
    }

    #[test]
    fn sharp_image_scores_above_blurry() {
        let sharp = sharpness_score(&gradient_image());
        let blurry = sharpness_score(&flat_image());
        assert!(sharp > REJECT_SHARPNESS, "sharp={sharp}");
        assert!(blurry < REJECT_SHARPNESS, "blurry={blurry}");
    }

    #[test]
    fn flat_image_has_no_faces() {
        assert_eq!(count_faces(&flat_image()), 0);
    }

    #[test]
    fn missing_file_sorts_regular() {
        assert_eq!(
            classify_output(Path::new("/definitely/not/here.jpg")),
            SortClass::Regular
        );
    }
}

/// Write a black-and-white copy of a corrected photo: same edit, monochrome.
/// Weighted luma mix (not flat desaturation) plus a gentle contrast curve so
/// the B&W version keeps the punch of the colour edit.
pub fn bw_copy(src: &Path, dst: &Path, quality: u8) -> Result<(), String> {
    let img = image::open(src).map_err(|e| format!("B&W source unreadable: {e}"))?;
    let rgb = img.to_rgb8();
    let mut out = image::GrayImage::new(rgb.width(), rgb.height());
    for (x, y, p) in rgb.enumerate_pixels() {
        let [r, g, b] = p.0;
        let mut l = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
        // Gentle S-curve around 128 for punch.
        let t = l / 255.0;
        l = (t + 0.18 * (t * t * (3.0 - 2.0 * t) - t)) * 255.0;
        out.put_pixel(x, y, image::Luma([l.clamp(0.0, 255.0) as u8]));
    }
    out.save_with_format(dst, image::ImageFormat::Jpeg)
        .map_err(|e| format!("B&W write failed: {e}"))?;
    let _ = quality;
    Ok(())
}
