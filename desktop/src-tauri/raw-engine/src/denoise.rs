//! Automatic noise reduction (classical, not AI): multi-scale soft-threshold
//! shrinkage on luma and chroma, with the strength set from the noise level
//! measured on each photo. Clean photos are left alone; strong detail
//! (edges, pores, hair) survives because only small coefficients near the
//! measured noise floor are removed, and part of the original luma is kept
//! so skin never goes plastic.

use image::Rgb32FImage;

fn box_pass(src: &[f32], w: usize, h: usize, r: usize, horiz: bool) -> Vec<f32> {
    let mut out = vec![0.0f32; src.len()];
    let (len, lines) = if horiz { (w, h) } else { (h, w) };
    let idx = |line: usize, i: usize| if horiz { line * w + i } else { i * w + line };
    let r = r.min(len.saturating_sub(1)).max(1);
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let mut sum = 0.0f32;
        for k in 0..=(2 * r) {
            let i = (k as isize - r as isize).clamp(0, len as isize - 1) as usize;
            sum += src[idx(line, i)];
        }
        for i in 0..len {
            out[idx(line, i)] = sum * norm;
            let add = (i + r + 1).min(len - 1);
            let sub = (i as isize - r as isize).max(0) as usize;
            sum += src[idx(line, add)] - src[idx(line, sub)];
        }
    }
    out
}

/// Two box passes in each direction: close to a Gaussian, linear time.
fn blur(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let a = box_pass(src, w, h, r, true);
    let b = box_pass(&a, w, h, r, true);
    let c = box_pass(&b, w, h, r, false);
    box_pass(&c, w, h, r, false)
}

fn soft(v: f32, t: f32) -> f32 {
    let a = v.abs() - t;
    if a <= 0.0 { 0.0 } else { a * v.signum() }
}

/// Robust noise sigma (same units as the plane) from the Laplacian-type
/// operator, using a median so edges and texture do not dominate.
fn estimate_sigma(p: &[f32], w: usize, h: usize) -> f32 {
    if w < 8 || h < 8 {
        return 0.0;
    }
    let mut vals: Vec<f32> = Vec::new();
    let step = (((w * h) / 600_000).max(1) as f64).sqrt().ceil() as usize;
    let mut y = 1;
    while y < h - 1 {
        let mut x = 1;
        while x < w - 1 {
            let g = |dx: isize, dy: isize| p[((y as isize + dy) as usize) * w + (x as isize + dx) as usize];
            let v = g(-1, -1) + g(1, -1) + g(-1, 1) + g(1, 1) - 2.0 * (g(0, -1) + g(-1, 0) + g(1, 0) + g(0, 1)) + 4.0 * g(0, 0);
            vals.push(v.abs());
            x += step;
        }
        y += step;
    }
    if vals.is_empty() {
        return 0.0;
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // 40th percentile: biased toward flat areas (texture inflates the median)
    let q = vals[(vals.len() as f32 * 0.4) as usize];
    q / (0.5244 * 6.0) // 40th pct of |N(0,s)| = 0.5244 s; operator gain 6
}

/// Std of each wavelet band for unit white noise, measured once on a
/// synthetic plane with the same decomposition, so thresholds match.
fn band_factors(radii: &[usize]) -> Vec<f32> {
    let (w, h) = (192usize, 192usize);
    let mut s: u32 = 12345;
    let mut next = || {
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        (s >> 8) as f32 / (1u32 << 24) as f32
    };
    // sum of 12 uniforms - 6 ~ N(0,1)
    let noise: Vec<f32> = (0..w * h).map(|_| (0..12).map(|_| next()).sum::<f32>() - 6.0).collect();
    let mut cur = noise;
    let mut out = Vec::new();
    for &r in radii {
        let low = blur(&cur, w, h, r);
        let m = 16;
        let mut acc = 0.0f64;
        let mut n = 0.0f64;
        for y in m..h - m {
            for x in m..w - m {
                let d = (cur[y * w + x] - low[y * w + x]) as f64;
                acc += d * d;
                n += 1.0;
            }
        }
        out.push((acc / n).sqrt() as f32);
        cur = low;
    }
    out
}

fn shrink_plane(p: &mut Vec<f32>, w: usize, h: usize, sigma: f32, ks: &[f32], radii: &[usize], shadow_w: &[f32], factors: &[f32]) {
    let mut cur = p.clone();
    let mut bands: Vec<Vec<f32>> = Vec::new();
    for (l, &r) in radii.iter().enumerate() {
        let low = blur(&cur, w, h, r);
        let t0 = ks[l] * sigma * factors[l];
        let mut band = vec![0.0f32; cur.len()];
        for i in 0..cur.len() {
            let d = cur[i] - low[i];
            band[i] = soft(d, t0 * shadow_w[i]);
        }
        bands.push(band);
        cur = low;
    }
    for i in 0..p.len() {
        let mut v = cur[i];
        for b in &bands {
            v += b[i];
        }
        p[i] = v;
    }
}

/// Denoise in place. Returns the estimated luma noise sigma (0..1 scale).
pub fn denoise(rgb: &mut Rgb32FImage) -> f32 {
    let (w, h) = (rgb.width() as usize, rgb.height() as usize);
    if w < 32 || h < 32 {
        return 0.0;
    }
    let n = w * h;
    let mut yp = vec![0.0f32; n];
    let mut cb = vec![0.0f32; n];
    let mut cr = vec![0.0f32; n];
    for (i, p) in rgb.pixels().enumerate() {
        let [r, g, b] = p.0;
        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        yp[i] = y;
        cb[i] = (b - y) * 0.564;
        cr[i] = (r - y) * 0.713;
    }
    let sig_y = estimate_sigma(&yp, w, h);
    let sig_cb = estimate_sigma(&cb, w, h);
    let sig_cr = estimate_sigma(&cr, w, h);
    // strength 0 on clean photos, full by sigma ~0.025 (about ISO 6400 look)
    let s_y = ((sig_y - 0.002) / 0.016).clamp(0.0, 1.0);
    let s_c = ((sig_cb.max(sig_cr) - 0.001) / 0.010).clamp(0.0, 1.0);
    if s_y <= 0.0 && s_c <= 0.0 {
        return sig_y;
    }
    // Shadows carry more noise after exposure gain: allow a bit more there,
    // and be gentler in bright areas where detail is clean.
    let coarse = blur(&yp, w, h, 8);
    let shadow_w: Vec<f32> = coarse.iter().map(|&c| 0.65 + 0.8 * (1.0 - (c * 2.0).clamp(0.0, 1.0))).collect();

    let y_orig = yp.clone();
    if s_y > 0.0 {
        let radii = [1usize, 2, 4, 8];
        let f = band_factors(&radii);
        let ks = [2.2 * s_y, 1.8 * s_y, 1.2 * s_y, 0.7 * s_y];
        shrink_plane(&mut yp, w, h, sig_y, &ks, &radii, &shadow_w, &f);
        // keep some original so skin texture stays natural, not plastic
        for i in 0..n {
            yp[i] = yp[i] * 0.88 + y_orig[i] * 0.12;
        }
    }
    if s_c > 0.0 {
        let radii = [1usize, 2, 4, 8, 16];
        let f = band_factors(&radii);
        let ks = [3.0 * s_c, 3.0 * s_c, 2.6 * s_c, 2.2 * s_c, 1.8 * s_c];
        let flat = vec![1.0f32; n];
        shrink_plane(&mut cb, w, h, sig_cb.max(1e-4), &ks, &radii, &flat, &f);
        shrink_plane(&mut cr, w, h, sig_cr.max(1e-4), &ks, &radii, &flat, &f);
    }
    for (i, p) in rgb.pixels_mut().enumerate() {
        let y = yp[i];
        let b = y + cb[i] / 0.564;
        let r = y + cr[i] / 0.713;
        let g = (y - 0.299 * r - 0.114 * b) / 0.587;
        p.0 = [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)];
    }
    sig_y
}
