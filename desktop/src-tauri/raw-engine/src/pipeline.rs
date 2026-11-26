//! Smart Basic auto-correction: gentle exposure and contrast recovery.
//!
//! Honest scope: these are deterministic histogram heuristics, not AI.
//! No skin smoothing, no generative changes, no creative relighting.

use image::{DynamicImage, Rgb32FImage};

const MEDIAN_TARGET: f32 = 0.45;
const GAIN_MIN: f32 = 0.5;
const GAIN_MAX: f32 = 2.0;

fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f32 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Exposure/contrast decision derived from the image's luma histogram.
/// Kept as a struct so unit tests can assert the decisions directly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CorrectionPlan {
    pub gain: f32,
    pub black_point: f32,
    pub gamma: f32,
    pub contrast_mix: f32,
    /// Per-channel white-balance multipliers (r, g, b). 1.0 = keep as shot.
    pub wb: (f32, f32, f32),
    /// Skin-hue nudge strength: 0 = no skin detected / no nudge.
    pub skin_warmth: f32,
    /// Mild saturation multiplier for dull photos (1.0 = unchanged).
    pub saturation: f32,
}

/// Hue (degrees) of an RGB pixel in 0..1 floats; None for near-gray.
fn hue_degrees(r: f32, g: f32, b: f32) -> Option<f32> {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if delta < 0.03 {
        return None;
    }
    let h = if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    Some(h.rem_euclid(360.0))
}

/// Heuristic skin test: red-dominant, moderately saturated, mid luma.
/// Good enough to warm portraits gently; not a face detector.
fn is_skinish(r: f32, g: f32, b: f32) -> bool {
    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if !(0.12..=0.85).contains(&luma) || r <= g || g < b * 0.9 {
        return false;
    }
    match hue_degrees(r, g, b) {
        Some(h) => (5.0..=55.0).contains(&h),
        None => false,
    }
}

pub struct ChannelStats {
    pub mean_r: f32,
    pub mean_g: f32,
    pub mean_b: f32,
    pub mean_chroma: f32,
    pub skin_hue_median: Option<f32>,
}

pub fn channel_stats(rgb: &Rgb32FImage) -> ChannelStats {
    let total = (rgb.width() as usize) * (rgb.height() as usize);
    let step = (total / 200_000).max(1);
    let (mut sr, mut sg, mut sb, mut sc, mut n) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0usize);
    let mut skin_hues: Vec<f32> = Vec::new();
    for (i, pixel) in rgb.pixels().enumerate() {
        if i % step != 0 {
            continue;
        }
        let [r, g, b] = pixel.0;
        sr += r as f64;
        sg += g as f64;
        sb += b as f64;
        sc += (r.max(g).max(b) - r.min(g).min(b)) as f64;
        n += 1;
        if is_skinish(r, g, b) {
            if let Some(h) = hue_degrees(r, g, b) {
                skin_hues.push(h);
            }
        }
    }
    if n == 0 {
        return ChannelStats { mean_r: 0.0, mean_g: 0.0, mean_b: 0.0, mean_chroma: 0.0, skin_hue_median: None };
    }
    let skin_hue_median = if skin_hues.len() * 200 >= n {
        skin_hues.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Some(skin_hues[skin_hues.len() / 2])
    } else {
        None
    };
    ChannelStats {
        mean_r: (sr / n as f64) as f32,
        mean_g: (sg / n as f64) as f32,
        mean_b: (sb / n as f64) as f32,
        mean_chroma: (sc / n as f64) as f32,
        skin_hue_median,
    }
}

/// White balance: partial gray-world. The stronger the measured cast, the
/// more we correct, but never to full neutral (that kills mood and warmth).
pub fn white_balance_gains(stats: &ChannelStats) -> (f32, f32, f32) {
    let avg = (stats.mean_r + stats.mean_g + stats.mean_b) / 3.0;
    if avg < 1e-4 {
        return (1.0, 1.0, 1.0);
    }
    let cast = ((stats.mean_r - avg).abs() + (stats.mean_b - avg).abs()) / avg;
    let strength = (cast * 2.0).clamp(0.10, 0.65);
    let warm_bias = 1.0 + 0.06 * strength; // stop short of clinical neutral
    let mut r = 1.0 + strength * (avg * warm_bias / stats.mean_r.max(1e-4) - 1.0);
    let mut g = 1.0 + strength * (avg / stats.mean_g.max(1e-4) - 1.0);
    let mut b = 1.0 + strength * (avg / warm_bias / stats.mean_b.max(1e-4) - 1.0);
    r = r.clamp(0.8, 1.25);
    g = g.clamp(0.85, 1.2);
    b = b.clamp(0.75, 1.25);
    (r, g, b)
}

pub fn plan_from_lumas(sorted_lumas: &[f32]) -> CorrectionPlan {
    let p01 = percentile(sorted_lumas, 0.01);
    let p50 = percentile(sorted_lumas, 0.50);
    let p99 = percentile(sorted_lumas, 0.99);

    let gain = if p50 > 1e-4 {
        (MEDIAN_TARGET / p50).clamp(GAIN_MIN, GAIN_MAX)
    } else {
        GAIN_MAX
    };

    // Small black-point subtraction so shadows keep depth.
    let black_point = (p01 * gain * 0.5).clamp(0.0, 0.05);

    // Dull images (no real whites even after gain) get a gentle gamma lift;
    // otherwise leave tone mapping to gain + highlight shoulder.
    let gamma = if p99 * gain < 0.85 { 0.85 } else { 1.0 };

    // Narrow histogram -> add some contrast; already wide -> almost none.
    let spread = (p99 - p01).clamp(0.0, 1.0);
    let contrast_mix = (0.30 - spread).clamp(0.02, 0.16);

    CorrectionPlan {
        gain,
        black_point,
        gamma,
        contrast_mix,
        wb: (1.0, 1.0, 1.0),
        skin_warmth: 0.0,
        saturation: 1.0,
    }
}

/// Full per-photo plan: exposure from the luma histogram, white balance and
/// warmth/skin nudges from channel statistics.
pub fn plan_for_image(rgb: &Rgb32FImage) -> CorrectionPlan {
    let lumas = sample_lumas(rgb);
    let mut plan = plan_from_lumas(&lumas);
    let stats = channel_stats(rgb);
    plan.wb = white_balance_gains(&stats);
    // Skin: nudge median skin hue toward a warm natural ~38 deg, max +-4 deg.
    if let Some(h) = stats.skin_hue_median {
        plan.skin_warmth = ((38.0 - h) / 38.0 * 0.10).clamp(-0.04, 0.04);
    }
    // Dull photos get a touch of saturation, lively ones stay put.
    plan.saturation = (1.0 + (0.10 - stats.mean_chroma) * 0.6).clamp(1.0, 1.07);
    plan
}

fn smoothstep(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

pub fn apply_plan(rgb: &mut Rgb32FImage, plan: &CorrectionPlan) {
    for pixel in rgb.pixels_mut() {
        let [r0, g0, b0] = pixel.0;
        // White balance.
        let mut r = r0 * plan.wb.0;
        let mut g = g0 * plan.wb.1;
        let mut b = b0 * plan.wb.2;
        // Skin warmth nudge (only where skin was detected).
        if plan.skin_warmth != 0.0 && is_skinish(r, g, b) {
            r *= 1.0 + plan.skin_warmth;
            b *= 1.0 - plan.skin_warmth;
        }
        // Gentle saturation around the pixel's own gray point.
        if plan.saturation > 1.0 {
            let gray = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            r = gray + (r - gray) * plan.saturation;
            g = gray + (g - gray) * plan.saturation;
            b = gray + (b - gray) * plan.saturation;
        }
        let channels = [r, g, b];
        for (i, channel) in pixel.0.iter_mut().enumerate() {
            // Exposure gain, small black-point floor, optional dull-image lift.
            let mut x = (channels[i] * plan.gain - plan.black_point).max(0.0);
            x = x.powf(plan.gamma);
            // Soft highlight shoulder: eases into 1.0 instead of clipping.
            if x > 0.95 {
                x = 0.95 + 0.05 * (1.0 - (-(x - 0.95) / 0.05).exp());
            }
            let curved = x + plan.contrast_mix * (smoothstep(x) - x);
            *channel = curved.clamp(0.0, 1.0);
        }
    }
}

/// Collect luma samples (Rec.709) with stride sampling so huge files stay fast.
pub fn sample_lumas(rgb: &Rgb32FImage) -> Vec<f32> {
    let (w, h) = rgb.dimensions();
    let total = (w as usize) * (h as usize);
    if total == 0 {
        return Vec::new();
    }
    let step = (total / 200_000).max(1);
    let mut lumas = Vec::with_capacity(total / step + 1);
    for (i, pixel) in rgb.pixels().enumerate() {
        if i % step != 0 {
            continue;
        }
        let [r, g, b] = pixel.0;
        lumas.push(0.2126 * r + 0.7152 * g + 0.0722 * b);
    }
    lumas.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    lumas
}

pub fn auto_correct(image: &DynamicImage) -> DynamicImage {
    let mut rgb = image.to_rgb32f();
    if rgb.width() == 0 || rgb.height() == 0 {
        return image.clone();
    }
    let plan = plan_for_image(&rgb);
    apply_plan(&mut rgb, &plan);
    DynamicImage::ImageRgb32F(rgb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, RgbImage};

    fn uniform_lumas(value: f32, n: usize) -> Vec<f32> {
        vec![value; n]
    }

    #[test]
    fn dark_image_gets_brightened() {
        let plan = plan_from_lumas(&uniform_lumas(0.2, 1000));
        assert!(plan.gain > 1.5, "dark image should get strong gain, got {}", plan.gain);
    }

    #[test]
    fn bright_image_gets_dimmed() {
        let plan = plan_from_lumas(&uniform_lumas(0.8, 1000));
        assert!(plan.gain < 0.7, "bright image should be dimmed, got {}", plan.gain);
    }

    #[test]
    fn well_exposed_image_gain_near_one() {
        let plan = plan_from_lumas(&uniform_lumas(0.45, 1000));
        assert!((plan.gain - 1.0).abs() < 0.05);
    }

    #[test]
    fn gain_is_clamped_for_extremes() {
        let plan = plan_from_lumas(&uniform_lumas(0.001, 100));
        assert!(plan.gain <= GAIN_MAX);
        let plan = plan_from_lumas(&uniform_lumas(0.999, 100));
        assert!(plan.gain >= GAIN_MIN);
    }

    #[test]
    fn apply_plan_never_produces_out_of_range() {
        let mut img = Rgb32FImage::new(4, 4);
        for (i, p) in img.pixels_mut().enumerate() {
            *p = image::Rgb([i as f32 / 16.0, 0.5, 1.0 - i as f32 / 16.0]);
        }
        let plan = CorrectionPlan { gain: 2.0, black_point: 0.05, gamma: 1.0, contrast_mix: 0.2, wb: (1.0, 1.0, 1.0), skin_warmth: 0.0, saturation: 1.0 };
        apply_plan(&mut img, &plan);
        for p in img.pixels() {
            for c in p.0 {
                assert!((0.0..=1.0).contains(&c));
            }
        }
    }

    #[test]
    fn midtones_stay_midtones_for_balanced_image() {
        // A mid-gray pixel in a well-exposed image should move very little.
        let mut img = Rgb32FImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgb([0.45, 0.45, 0.45]));
        let plan = plan_from_lumas(&uniform_lumas(0.45, 1000));
        apply_plan(&mut img, &plan);
        let v = img.get_pixel(0, 0).0[0];
        assert!((v - 0.45).abs() < 0.12, "mid gray drifted to {v}");
    }

    #[test]
    fn empty_image_is_untouched() {
        let img = DynamicImage::ImageRgb8(RgbImage::new(0, 0));
        let out = auto_correct(&img);
        assert_eq!(out.dimensions(), (0, 0));
    }
}
