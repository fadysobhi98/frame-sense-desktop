use std::path::Path;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = Path::new(&a[1]);
    let img = framesense_raw::decode_to_dynamic(src).unwrap();
    let small = img.resize(500, 500, image::imageops::FilterType::Triangle);
    small.to_rgb8().save(format!("{}_raw.jpg", a[2])).unwrap();
    for (i, l) in framesense_raw::pipeline::Look::all().iter().enumerate() {
        let c = framesense_raw::pipeline::auto_correct(&small, *l);
        c.to_rgb8().save(format!("{}_{}.jpg", a[2], i)).unwrap();
    }
}
