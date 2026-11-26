use framesense_raw::pipeline::{auto_correct, Look};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    for f in &a[2..] {
        let t = std::time::Instant::now();
        match framesense_raw::decode_to_dynamic(std::path::Path::new(f)) {
            Ok(img) => {
                let (w, h) = (img.width(), img.height());
                let td = t.elapsed();
                let name = std::path::Path::new(f).file_stem().unwrap().to_string_lossy().to_string();
                let t2 = std::time::Instant::now();
                let full = auto_correct(&img, Look::Auto);
                let ta = t2.elapsed();
                let small0 = img.resize(700, 700, image::imageops::FilterType::Triangle);
                small0.to_rgb8().save(format!("{}/{}_raw.jpg", a[1], name)).unwrap();
                full.resize(700, 700, image::imageops::FilterType::Triangle).to_rgb8().save(format!("{}/{}_auto.jpg", a[1], name)).unwrap();
                println!("{name} {w}x{h} decode {td:?} auto {ta:?}");
            }
            Err(e) => println!("{f} FAIL {e}"),
        }
    }
}
