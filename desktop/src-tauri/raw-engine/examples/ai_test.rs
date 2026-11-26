use framesense_raw::pipeline::{auto_correct, Look};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let img = framesense_raw::decode_to_dynamic(std::path::Path::new(&a[1])).unwrap();
    let t = std::time::Instant::now();
    framesense_raw::lut3::configure(false, 0.0, "/tmp/landmark-proto/libonnxruntime.so".into(), "../models".into());
    auto_correct(&img, Look::Auto).to_rgb8().save(format!("{}_off.jpg", a[2])).unwrap();
    let t1 = t.elapsed();
    framesense_raw::lut3::configure(true, 0.6, "/tmp/landmark-proto/libonnxruntime.so".into(), "../models".into());
    auto_correct(&img, Look::Auto).to_rgb8().save(format!("{}_on.jpg", a[2])).unwrap();
    println!("off {:?} total {:?}", t1, t.elapsed());
}
