use std::ffi::CString;
use base64::Engine;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let models = CString::new("/tmp/models_eye").unwrap();
    for f in &a[2..] {
        let stem = std::path::Path::new(f).file_stem().unwrap().to_string_lossy().to_string();
        let src = CString::new(f.as_str()).unwrap();
        let mut res = vec![];
        for look in [255u8, 9u8] {
            let p = framesense_raw::fs_raw_preview_jpeg(src.as_ptr(), 1400, look, 0, std::ptr::null(), models.as_ptr());
            let s = unsafe { std::ffi::CStr::from_ptr(p) }.to_str().unwrap().to_string();
            let bytes = base64::engine::general_purpose::STANDARD.decode(s).unwrap();
            std::fs::write(format!("{}/{stem}_prev{look}.jpg", a[1]), bytes).unwrap();
            res.push(framesense_raw::fs_raw_preview_clip());
        }
        println!("{stem} clip before {:.1}% after {:.1}%", res[0], res[1]);
    }
}
