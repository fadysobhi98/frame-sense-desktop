use std::path::Path;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (src, dst, ort_lib, model_dir) = (&a[1], &a[2], &a[3], &a[4]);
    let look: u8 = a.get(5).map(|s| s.parse().unwrap()).unwrap_or(1);
    let look = match look { 0 => framesense_raw::pipeline::Look::Natural, 2 => framesense_raw::pipeline::Look::Golden, _ => framesense_raw::pipeline::Look::Warm };
    let t0 = std::time::Instant::now();
    // without retouch
    let (w, h) = framesense_raw::auto_correct_file_v2(Path::new(src), Path::new(&format!("{dst}")), 92, look, 0, None, None).unwrap();
    println!("plain {}x{} in {:?}", w, h, t0.elapsed());
    let t1 = std::time::Instant::now();
    let dst_r = dst.replace(".jpg", "_retouch.jpg");
    framesense_raw::auto_correct_file_v2(Path::new(src), Path::new(&dst_r), 92, look, 1, Some(Path::new(ort_lib)), Some(Path::new(model_dir))).unwrap();
    println!("retouch in {:?}", t1.elapsed());
    // preview api
    let t2 = std::time::Instant::now();
    let b64 = framesense_raw::preview_jpeg_base64(Path::new(src), 1200, look, 1, Some(Path::new(ort_lib)), Some(Path::new(model_dir))).unwrap();
    println!("preview b64 len {} in {:?}", b64.len(), t2.elapsed());
}
