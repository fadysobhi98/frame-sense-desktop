use std::path::Path;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let retouch: u8 = a.get(4).map(|v| v.parse().unwrap()).unwrap_or(0);
    let ort = Path::new("/tmp/landmark-proto/libonnxruntime.so");
    let t = std::time::Instant::now();
    let r = framesense_raw::auto_correct_file_v2(Path::new(&a[1]), Path::new(&a[3]), 92, framesense_raw::pipeline::Look::Warm, retouch, Some(ort), Some(Path::new(&a[2])));
    println!("{:?} {:?}", r, t.elapsed());
}
