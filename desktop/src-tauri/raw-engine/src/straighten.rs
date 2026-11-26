//! Conservative horizon straightening: detect dominant near-horizontal
//! lines, and only rotate when they agree. Minimal center crop afterward.
//! Heuristic, not AI: when in doubt it leaves the photo alone.

use image::{DynamicImage, Rgb, Rgb32FImage};
use imageproc::edges::canny;
use imageproc::hough::{detect_lines, LineDetectionOptions};

const MAX_TILT_DEG: f32 = 5.0;
const MIN_TILT_DEG: f32 = 0.4;

fn bilinear(img: &Rgb32FImage, x: f32, y: f32) -> Rgb<f32> {
    let w = img.width() as f32 - 1.0;
    let h = img.height() as f32 - 1.0;
    let x = x.clamp(0.0, w);
    let y = y.clamp(0.0, h);
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = (x0.min(img.width() - 1), (y0 + 1).min(img.height() - 1));
    let x1 = (x0 + 1).min(img.width() - 1);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let p00 = img.get_pixel(x0, y0).0;
    let p10 = img.get_pixel(x1, y0).0;
    let p01 = img.get_pixel(x0, y1).0;
    let p11 = img.get_pixel(x1, y1).0;
    let mut out = [0.0f32; 3];
    for c in 0..3 {
        let top = p00[c] * (1.0 - fx) + p10[c] * fx;
        let bot = p01[c] * (1.0 - fx) + p11[c] * fx;
        out[c] = top * (1.0 - fy) + bot * fy;
    }
    Rgb(out)
}

pub fn rotate_centered(img: &Rgb32FImage, theta_rad: f32) -> Rgb32FImage {
    let (w, h) = img.dimensions();
    let (cx, cy) = ((w as f32 - 1.0) / 2.0, (h as f32 - 1.0) / 2.0);
    let (sin, cos) = theta_rad.sin_cos();
    let mut out = Rgb32FImage::new(w, h);
    for (x, y, p) in out.enumerate_pixels_mut() {
        let dx = x as f32 - cx;
        let dy = y as f32 - cy;
        let sx = cx + dx * cos + dy * sin;
        let sy = cy - dx * sin + dy * cos;
        *p = bilinear(img, sx, sy);
    }
    out
}

pub fn crop_center(img: &Rgb32FImage, new_w: u32, new_h: u32) -> Rgb32FImage {
    let (w, h) = img.dimensions();
    let new_w = new_w.min(w);
    let new_h = new_h.min(h);
    let x0 = (w - new_w) / 2;
    let y0 = (h - new_h) / 2;
    image::imageops::crop_imm(img, x0, y0, new_w, new_h).to_image()
}

/// Detect the dominant horizon tilt in degrees; None when not confident.
pub fn detect_tilt(img: &Rgb32FImage) -> Option<f32> {
    let gray = DynamicImage::ImageRgb32F(img.clone()).to_luma8();
    let (w, h) = gray.dimensions();
    let scale = 360.0 / w.max(h) as f32;
    let gray = if scale < 1.0 {
        image::imageops::resize(
            &gray,
            (w as f32 * scale) as u32,
            (h as f32 * scale) as u32,
            image::imageops::FilterType::Triangle,
        )
    } else {
        gray
    };
    let edges = canny(&gray, 50.0, 120.0);
    let options = LineDetectionOptions {
        vote_threshold: 60,
        suppression_radius: 6,
    };
    let lines = detect_lines(&edges, options);
    // Horizontal lines have normals near 90 degrees.
    let mut tilts: Vec<f32> = lines
        .iter()
        .take(24)
        .map(|l| l.angle_in_degrees as f32)
        .filter(|a| (75.0..=105.0).contains(a))
        .map(|a| 90.0 - a)
        .collect();
    if tilts.is_empty() {
        return None;
    }
    tilts.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = tilts[tilts.len() / 2];
    if tilts.len() >= 3 && (tilts[tilts.len() - 1] - tilts[0]) > 3.0 {
        return None; // lines disagree; not a clean horizon signal
    }
    let tilt = median;
    if tilt.abs() < MIN_TILT_DEG || tilt.abs() > MAX_TILT_DEG {
        return None;
    }
    Some(tilt)
}

/// Straighten if a confident small tilt is found; otherwise return unchanged.
pub fn auto_straighten(img: &Rgb32FImage) -> Rgb32FImage {
    let Some(tilt_deg) = detect_tilt(img) else {
        return img.clone();
    };
    let theta = tilt_deg.to_radians();
    let rotated = rotate_centered(img, theta);
    let (w, h) = img.dimensions();
    let cos = theta.abs().cos();
    let sin = theta.abs().sin();
    let new_w = (w as f32 * cos - h as f32 * sin) as u32;
    let new_h = (h as f32 * cos - w as f32 * sin) as u32;
    if new_w < (w as f32 * 0.85) as u32 || new_h < (h as f32 * 0.85) as u32 {
        return img.clone(); // crop would be too aggressive; leave it alone
    }
    crop_center(&rotated, new_w, new_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> Rgb32FImage {
        let mut img = Rgb32FImage::new(w, h);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = Rgb([x as f32 / w as f32, y as f32 / h as f32, 0.5]);
        }
        img
    }

    #[test]
    fn featureless_image_is_untouched() {
        let img = gradient(200, 150);
        let out = auto_straighten(&img);
        assert_eq!(out.dimensions(), img.dimensions());
    }

    #[test]
    fn rotation_then_crop_keeps_expected_size() {
        let img = gradient(400, 300);
        let rotated = rotate_centered(&img, 3.0f32.to_radians());
        let theta = 3.0f32.to_radians().abs();
        let new_w = (400.0 * theta.cos() - 300.0 * theta.sin()) as u32;
        let new_h = (300.0 * theta.cos() - 400.0 * theta.sin()) as u32;
        let cropped = crop_center(&rotated, new_w, new_h);
        assert_eq!(cropped.dimensions(), (new_w, new_h));
        assert!(new_w > 340 && new_h > 240, "crop must be minimal");
    }

    #[test]
    fn tilted_horizon_line_is_detected() {
        // Synthetic photo: bright sky over dark ground, tilted 3 degrees.
        let (w, h) = (400u32, 300u32);
        let mut img = Rgb32FImage::new(w, h);
        let theta = 3.0f32.to_radians();
        for (x, y, p) in img.enumerate_pixels_mut() {
            let horizon = h as f32 / 2.0 + (x as f32 - w as f32 / 2.0) * theta.tan();
            *p = if (y as f32) < horizon {
                Rgb([0.8, 0.85, 0.95])
            } else {
                Rgb([0.1, 0.2, 0.12])
            };
        }
        let tilt = detect_tilt(&img).expect("synthetic 3-degree horizon should be found");
        assert!((tilt.abs() - 3.0).abs() < 0.8, "measured tilt {tilt}");
    }
}
