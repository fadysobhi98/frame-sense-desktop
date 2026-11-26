use framesense_raw::pipeline::{auto_correct, Look};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let img = framesense_raw::decode_to_dynamic(std::path::Path::new(&a[1])).unwrap();
    let small = img.resize(300, 300, image::imageops::FilterType::Triangle);
    for i in 0..24u8 {
        auto_correct(&small, Look::from_u8(10 + i)).to_rgb8().save(format!("{}_{}.jpg", a[2], i)).unwrap();
    }
}
