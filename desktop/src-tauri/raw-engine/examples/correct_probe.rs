use std::path::Path;
fn main() {
    for arg in std::env::args().skip(1) {
        let src = Path::new(&arg);
        let dst = std::env::temp_dir().join(format!("fs-out-{}.jpg", src.file_stem().unwrap().to_string_lossy()));
        match framesense_raw::auto_correct_file(src, &dst, 88) {
            Ok((w, h)) => println!("{} -> {}x{} ({})", arg, w, h, dst.display()),
            Err(e) => println!("{} -> ERROR {}", arg, e),
        }
    }
}
