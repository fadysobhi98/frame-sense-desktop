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
    }
}

fn smoothstep(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

pub fn apply_plan(rgb: &mut Rgb32FImage, plan: &CorrectionPlan) {
    for pixel in rgb.pixels_mut() {
        for channel in pixel.0.iter_mut() {
            // Exposure gain, small black-point floor, optional dull-image lift.
            let mut x = (*channel * plan.gain - plan.black_point).max(0.0);
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
    let lumas = sample_lumas(&rgb);
    if lumas.is_empty() {
        return image.clone();
    }
    let plan = plan_from_lumas(&lumas);
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
        let plan = CorrectionPlan { gain: 2.0, black_point: 0.05, gamma: 1.0, contrast_mix: 0.2 };
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
