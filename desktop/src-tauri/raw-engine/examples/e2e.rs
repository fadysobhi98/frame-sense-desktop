use std::ffi::CString;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let out = &a[1];
    let models = CString::new("/tmp/models_eye").unwrap();
    for f in &a[2..] {
        let stem = std::path::Path::new(f).file_stem().unwrap().to_string_lossy().to_string();
        let src = CString::new(f.as_str()).unwrap();
        let dst = CString::new(format!("{out}/{stem}.jpg")).unwrap();
        let t = std::time::Instant::now();
        let code = framesense_raw::fs_raw_auto_correct_file_v2(src.as_ptr(), dst.as_ptr(), 92, 9, 0, std::ptr::null(), models.as_ptr());
        println!("{stem} code {code} {:?}", t.elapsed());
        if let Ok(img) = framesense_raw::decode_to_dynamic(std::path::Path::new(f)) {
            img.resize(800, 800, image::imageops::FilterType::Triangle).to_rgb8().save(format!("{out}/{stem}_src.png")).unwrap();
        }
    }
}
