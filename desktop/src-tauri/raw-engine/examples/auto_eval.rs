use framesense_raw::pipeline::{auto_correct, Look};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    for f in &a[1..] {
        let img = framesense_raw::decode_to_dynamic(std::path::Path::new(f)).unwrap();
        let img = img.resize(1000, 1000, image::imageops::FilterType::Triangle);
        auto_correct(&img, Look::Auto).to_rgb8().save(f.replace(".jpg", "_auto.jpg")).unwrap();
    }
}
