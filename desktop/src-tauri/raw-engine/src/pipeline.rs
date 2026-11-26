//! Smart Basic auto-correction: gentle exposure and contrast recovery.
//!
//! Honest scope: these are deterministic histogram heuristics, not AI.
//! No skin smoothing, no generative changes, no creative relighting.

use image::{DynamicImage, Rgb32FImage};

/// Session tone presets (the app's "session tones"). Honest scope: still
/// deterministic heuristic grades - per-channel gains, split-toning and
/// tone-curve shaping, stronger at higher levels, never a creative
/// restyle, no AI and no LUTs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Natural,
    Warm,
    Golden,
    PeachGold,
    AmberIndoor,
    BrightAiry,
    Cinematic,
}

/// Grade parameters behind a session tone.
#[derive(Debug, Clone, Copy)]
pub struct LookParams {
    /// Warm white-balance grade strength: 0 = none, 1 = strongest.
    pub warmth: f32,
    /// Orange split-tone multiplier on mids/highlights.
    pub split: f32,
    /// How far the split-tone reaches into shadows (0 = mids only).
    pub split_shadow: f32,
    /// Extra saturation on top of the adaptive amount.
    pub saturation_boost: f32,
    /// Peach/magenta tint added with the warm grade (0 = none).
    pub tint_r: f32,
    pub tint_g: f32,
    /// Faded-blacks floor (film look): 0 = none.
    pub black_floor: f32,
    /// Creamy highlight shoulder strength: 0 = standard shoulder.
    pub cream: f32,
    /// Contrast bias added to the adaptive contrast (can be negative).
    pub contrast_bias: f32,
}

impl Look {
    pub fn from_u8(v: u8) -> Look {
        match v {
            0 => Look::Natural,
            2 => Look::Golden,
            3 => Look::PeachGold,
            4 => Look::AmberIndoor,
            5 => Look::BrightAiry,
            6 => Look::Cinematic,
            _ => Look::Warm,
        }
    }
    /// All session tones, in menu order.
    pub fn all() -> [Look; 7] {
        [
            Look::Natural,
            Look::Warm,
            Look::Golden,
            Look::PeachGold,
            Look::AmberIndoor,
            Look::BrightAiry,
            Look::Cinematic,
        ]
    }
    fn params(self) -> LookParams {
        const BASE: LookParams = LookParams {
            warmth: 0.0,
            split: 1.0,
            split_shadow: 0.0,
            saturation_boost: 0.0,
            tint_r: 0.0,
            tint_g: 0.0,
            black_floor: 0.0,
            cream: 0.0,
            contrast_bias: 0.0,
        };
        match self {
            Look::Natural => LookParams { warmth: 0.15, split: 0.5, ..BASE },
            Look::Warm => LookParams { warmth: 0.7, saturation_boost: 0.03, ..BASE },
            Look::Golden => LookParams { warmth: 1.0, split: 1.15, saturation_boost: 0.06, ..BASE },
            // Peach-gold: warm with a soft peach tint (kaze-style glow)
            Look::PeachGold => LookParams { warmth: 0.8, tint_r: 0.020, tint_g: 0.006, saturation_boost: 0.04, cream: 0.4, ..BASE },
            // Amber indoor: deep warm for tungsten venues, split reaches shadows
            Look::AmberIndoor => LookParams { warmth: 0.9, split: 1.2, split_shadow: 0.35, saturation_boost: 0.04, ..BASE },
            // Bright airy: gentle warmth, lifted blacks, soft contrast (wedding)
            Look::BrightAiry => LookParams { warmth: 0.45, split: 0.6, saturation_boost: 0.02, black_floor: 0.025, cream: 0.6, contrast_bias: -0.03, ..BASE },
            // Cinematic: film-like faded blacks, creamy highlights, slightly
            // restrained saturation
            Look::Cinematic => LookParams { warmth: 0.55, split: 0.85, saturation_boost: -0.02, black_floor: 0.05, cream: 1.0, contrast_bias: -0.02, ..BASE },
        }
    }
}

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
    /// Warm grade strength from the session look (0..1).
    pub warmth: f32,
    /// Orange split-tone multiplier.
    pub split: f32,
    /// Split-tone reach into shadows (0..1).
    pub split_shadow: f32,
    /// Peach tint added with warmth.
    pub tint_r: f32,
    pub tint_g: f32,
    /// Faded blacks floor (0 = none).
    pub black_floor: f32,
    /// Creamy highlight shoulder strength.
    pub cream: f32,
    /// Additive lift for dark tones (backlit subjects), 0 = none.
    pub shadow_lift: f32,
    /// Highlight pull-down strength (bright sky / clothes), 0 = none.
    pub highlight_compress: f32,
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

    // Black-point subtraction so shadows keep depth and photos look punchy.
    let black_point = (p01 * gain * 0.9).clamp(0.01, 0.09);

    // Dull images (no real whites even after gain) get a gentle gamma lift;
    // otherwise leave tone mapping to gain + highlight shoulder.
    let gamma = if p99 * gain < 0.85 { 0.85 } else { 1.0 };

    let p10 = percentile(sorted_lumas, 0.10);

    // Backlit / deep-shadow photos: after exposure the dark tones still sit
    // near black, so lift them. Scaled by how far below 0.16 they land.
    let shadow_lift = ((0.16 - p10 * gain).max(0.0) * 0.9).min(0.14);

    // Bright scenes that still clip after exposure: pull the highlights
    // down so sky and white clothes keep texture instead of flat white.
    let clipped = (p99 * gain - 0.98).max(0.0);
    let highlight_compress = (clipped * 1.5).clamp(0.0, 0.35);

    // Narrow histogram -> add contrast; already wide -> still some punch.
    let spread = (p99 - p01).clamp(0.0, 1.0);
    let contrast_mix = (0.40 - spread).clamp(0.08, 0.25);

    CorrectionPlan {
        gain,
        black_point,
        gamma,
        contrast_mix,
        wb: (1.0, 1.0, 1.0),
        skin_warmth: 0.0,
        saturation: 1.0,
        warmth: 0.0,
        split: 1.0,
        split_shadow: 0.0,
        tint_r: 0.0,
        tint_g: 0.0,
        black_floor: 0.0,
        cream: 0.0,
        shadow_lift,
        highlight_compress,
    }
}

/// Full per-photo plan: exposure from the luma histogram, white balance and
/// warmth/skin nudges from channel statistics, graded by the session look.
pub fn plan_for_image(rgb: &Rgb32FImage, look: Look) -> CorrectionPlan {
    let lumas = sample_lumas(rgb);
    let mut plan = plan_from_lumas(&lumas);
    let stats = channel_stats(rgb);
    plan.wb = white_balance_gains(&stats);
    let lp = look.params();
    plan.warmth = lp.warmth;
    plan.split = lp.split;
    plan.split_shadow = lp.split_shadow;
    plan.tint_r = lp.tint_r;
    plan.tint_g = lp.tint_g;
    plan.black_floor = lp.black_floor;
    plan.cream = lp.cream;
    plan.contrast_mix = (plan.contrast_mix + lp.contrast_bias).clamp(0.05, 0.25);
    // Skin: nudge median skin hue toward a warm natural ~38 deg, max +-4 deg.
    if let Some(h) = stats.skin_hue_median {
        plan.skin_warmth = ((38.0 - h) / 38.0 * 0.10).clamp(-0.04, 0.04);
    }
    // Dull photos get a touch of saturation, lively ones stay put; the
    // chosen look adds a little more.
    plan.saturation = ((1.0 + (0.10 - stats.mean_chroma) * 0.6).clamp(1.0, 1.07)
        + lp.saturation_boost)
    .clamp(1.0, 1.13);
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
        // Warm grade: push toward golden-orange sun. Skin tones are
        // protected so faces stay natural instead of going neon orange.
        // A soft orange split-tone rides the mids/highlights (sunsets,
        // tungsten venues) and fades out in shadows.
        if plan.warmth > 0.0 {
            let skin = is_skinish(r, g, b);
            let w = if skin { plan.warmth * 0.45 } else { plan.warmth };
            r *= 1.0 + 0.16 * w;
            g *= 1.0 - 0.032 * w;
            b *= 1.0 - 0.185 * w;
            // optional peach tint riding the warm grade
            if plan.tint_r != 0.0 || plan.tint_g != 0.0 {
                r += plan.tint_r * w * (1.0 - r);
                g += plan.tint_g * w * (1.0 - g);
            }
            // orange split-tone on mids/highlights (optionally into shadows)
            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let lo = 0.30 - 0.15 * plan.split_shadow;
            let t = smoothstep(((luma - lo) / (0.55 + 0.15 * plan.split_shadow)).clamp(0.0, 1.0)) * w * plan.split;
            r += 0.055 * t * (1.0 - r);
            g += 0.012 * t * (1.0 - g);
            b -= 0.05 * t * b;
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
            // faded blacks (film floor) before gamma
            if plan.black_floor > 0.0 {
                x = plan.black_floor + x * (1.0 - plan.black_floor);
            }
            x = x.powf(plan.gamma);
            // Open up shadows (backlit subjects): lifts dark tones most,
            // fades out by mid-gray.
            if plan.shadow_lift > 0.0 && x < 0.35 {
                let mask = 1.0 - (x / 0.35);
                x += plan.shadow_lift * mask * mask;
            }
            // Pull down highlights so bright sky/clothes keep texture.
            if plan.highlight_compress > 0.0 && x > 0.80 {
                let t = x - 0.80;
                x = 0.80 + t / (1.0 + plan.highlight_compress * 8.0 * t);
            }
            // Soft highlight shoulder: eases into 1.0 instead of clipping.
            // "Cream" starts the shoulder earlier and stretches it wider.
            let sh = 0.95 - 0.10 * plan.cream.min(1.0);
            let sw = 0.05 + 0.05 * plan.cream.min(1.0);
            if x > sh {
                x = sh + sw * (1.0 - (-(x - sh) / sw).exp());
            }
            let curved = x + plan.contrast_mix * (smoothstep(x) - x);
            *channel = curved.clamp(0.0, 1.0);
        }
    }
}

/// Local tone mapping on a coarse luma grid: lifts locally dark regions
/// (faces against a bright sky) and settles locally bright ones, while
/// keeping the photo's own contrast. A basic-edit dodge and burn, still a
/// deterministic heuristic.
pub fn local_tone_map(rgb: &mut Rgb32FImage, strength: f32) {
    let (w, h) = rgb.dimensions();
    if w < 16 || h < 16 || strength <= 0.0 {
        return;
    }
    // Coarse grid of local mean luma (~48 cells on the long side).
    let gw = 48usize.min(w as usize);
    let gh = ((h as usize) * gw / (w as usize)).max(1);
    let mut grid = vec![0.0f64; gw * gh];
    let mut count = vec![0u32; gw * gh];
    let (wf, hf) = (w as f32, h as f32);
    // Stride only for building the local-mean grid (sampling is fine there);
    // the apply pass below must touch every pixel.
    let step = (((w as usize * h as usize) / 400_000).max(1)) as u32;
    for y in (0..h).step_by(step as usize) {
        for x in (0..w).step_by(step as usize) {
            let [r, g, b] = rgb.get_pixel(x, y).0;
            let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let gx = ((x as f32 / wf) * gw as f32).min(gw as f32 - 1.0) as usize;
            let gy = ((y as f32 / hf) * gh as f32).min(gh as f32 - 1.0) as usize;
            grid[gy * gw + gx] += l as f64;
            count[gy * gw + gx] += 1;
        }
    }
    let mut local: Vec<f32> = grid
        .iter()
        .zip(count.iter())
        .map(|(s, c)| if *c > 0 { (*s / *c as f64) as f32 } else { 0.0 })
        .collect();
    // Two box-blur passes so the map is smooth (no halos).
    for _ in 0..2 {
        let prev = local.clone();
        for gy in 0..gh {
            for gx in 0..gw {
                let mut sum = 0.0;
                let mut n = 0.0;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let xx = gx as i32 + dx;
                        let yy = gy as i32 + dy;
                        if xx >= 0 && yy >= 0 && (xx as usize) < gw && (yy as usize) < gh {
                            sum += prev[yy as usize * gw + xx as usize];
                            n += 1.0;
                        }
                    }
                }
                local[gy * gw + gx] = sum / n;
            }
        }
    }
    let bilinear = |fx: f32, fy: f32| -> f32 {
        let x0 = (fx.floor() as usize).min(gw - 1);
        let y0 = (fy.floor() as usize).min(gh - 1);
        let x1 = (x0 + 1).min(gw - 1);
        let y1 = (y0 + 1).min(gh - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let a = local[y0 * gw + x0] * (1.0 - tx) + local[y0 * gw + x1] * tx;
        let b = local[y1 * gw + x0] * (1.0 - tx) + local[y1 * gw + x1] * tx;
        a * (1.0 - ty) + b * ty
    };
    for y in 0..h {
        for x in 0..w {
            let p = rgb.get_pixel_mut(x, y);
            let l = 0.2126 * p.0[0] + 0.7152 * p.0[1] + 0.0722 * p.0[2];
            if l < 1e-4 {
                continue;
            }
            let loc = bilinear(
                (x as f32 / wf) * gw as f32 - 0.5,
                (y as f32 / hf) * gh as f32 - 0.5,
            );
            if loc < 1e-4 {
                continue;
            }
            // Filmic-ish target for the local mean: sqrt lifts shadows and
            // compresses highlights; blended by strength.
            let target = loc * (1.0 - strength) + strength * (loc.sqrt() * 0.9);
            let ratio = (target / loc).clamp(0.7, 1.6);
            for c in p.0.iter_mut() {
                *c *= ratio;
            }
        }
    }
}

/// Collect luma samples (Rec.709) with stride sampling so huge files stay fast.
/// Suggest a session tone from the photo's own brightness and colour.
/// Deterministic heuristic (medians and channel means), honestly labelled
/// as a suggestion in the UI - no AI scene recognition.
pub fn suggest_look(rgb: &Rgb32FImage, has_people: bool) -> (Look, &'static str) {
    let mut lumas = sample_lumas(rgb);
    if lumas.is_empty() {
        return (Look::Warm, "mixed-light photos");
    }
    lumas.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = lumas[lumas.len() / 2];
    let stats = channel_stats(rgb);
    let warm_deficit = stats.mean_b < stats.mean_r * 0.82;
    if median < 0.26 {
        if warm_deficit {
            (Look::AmberIndoor, "dark photos in warm/tungsten light")
        } else {
            (Look::Cinematic, "dark, moody photos")
        }
    } else if median > 0.58 {
        if has_people {
            (Look::BrightAiry, "bright photos with people")
        } else {
            (Look::Natural, "bright, well-lit photos")
        }
    } else if has_people {
        (Look::Warm, "photos with people in mixed light")
    } else {
        (Look::Warm, "mixed-light photos")
    }
}

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

pub fn auto_correct(image: &DynamicImage, look: Look) -> DynamicImage {
    let mut rgb = image.to_rgb32f();
    if rgb.width() == 0 || rgb.height() == 0 {
        return image.clone();
    }
    // Local tone first (on the as-shot distribution), then the graded plan
    // adapts to what remains.
    local_tone_map(&mut rgb, 0.5);
    let plan = plan_for_image(&rgb, look);
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
        let plan = CorrectionPlan { gain: 2.0, black_point: 0.05, gamma: 1.0, contrast_mix: 0.2, wb: (1.0, 1.0, 1.0), skin_warmth: 0.0, saturation: 1.0, warmth: 0.0, split: 1.0, split_shadow: 0.0, tint_r: 0.0, tint_g: 0.0, black_floor: 0.0, cream: 0.0, shadow_lift: 0.0, highlight_compress: 0.0 };
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
    fn warm_grade_visibly_warms_neutral_pixel() {
        for look in [Look::Warm, Look::Golden] {
            let mut img = Rgb32FImage::new(1, 1);
            img.put_pixel(0, 0, image::Rgb([0.45, 0.45, 0.45]));
            let plan = plan_for_image(&img, look);
            apply_plan(&mut img, &plan);
            let [r, _g, b] = img.get_pixel(0, 0).0;
            assert!(r > b + 0.03, "{look:?}: r={r} b={b}");
        }
        // Golden warms more than Warm.
        let mut a = Rgb32FImage::new(1, 1);
        a.put_pixel(0, 0, image::Rgb([0.45, 0.45, 0.45]));
        let pa = plan_for_image(&a, Look::Warm);
        apply_plan(&mut a, &pa);
        let mut b2 = Rgb32FImage::new(1, 1);
        b2.put_pixel(0, 0, image::Rgb([0.45, 0.45, 0.45]));
        let pb = plan_for_image(&b2, Look::Golden);
        apply_plan(&mut b2, &pb);
        let wa = a.get_pixel(0, 0).0[0] - a.get_pixel(0, 0).0[2];
        let wb = b2.get_pixel(0, 0).0[0] - b2.get_pixel(0, 0).0[2];
        assert!(wb > wa, "golden {wb} should warm more than warm {wa}");
    }

    #[test]
    fn backlit_histogram_gets_shadow_lift() {
        // Mostly dark subject (luma 0.05) with a bright sky strip (0.95).
        let mut lumas = vec![0.05f32; 700];
        lumas.extend(vec![0.95f32; 300]);
        lumas.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let plan = plan_from_lumas(&lumas);
        assert!(plan.shadow_lift > 0.03, "expected shadow lift, got {}", plan.shadow_lift);
    }

    #[test]
    fn clipping_histogram_gets_highlight_compress() {
        // Dark median pushes gain up; the bright tail then genuinely clips.
        let mut lumas = vec![0.2f32; 700];
        lumas.extend(vec![0.99f32; 300]);
        lumas.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let plan = plan_from_lumas(&lumas);
        assert!(plan.highlight_compress > 0.05, "expected highlight compression, got {}", plan.highlight_compress);
    }

    #[test]
    fn local_tone_lifts_dark_subject_on_bright_background() {
        // Left half dark (0.08), right half bright (0.9).
        let mut img = Rgb32FImage::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let v = if x < 32 { 0.08 } else { 0.9 };
                img.put_pixel(x, y, image::Rgb([v, v, v]));
            }
        }
        local_tone_map(&mut img, 0.5);
        let dark_after = img.get_pixel(10, 32).0[0];
        assert!(dark_after > 0.10, "dark region should be lifted, got {dark_after}");
    }

    #[test]
    fn empty_image_is_untouched() {
        let img = DynamicImage::ImageRgb8(RgbImage::new(0, 0));
        let out = auto_correct(&img, Look::Natural);
        assert_eq!(out.dimensions(), (0, 0));
    }
}
