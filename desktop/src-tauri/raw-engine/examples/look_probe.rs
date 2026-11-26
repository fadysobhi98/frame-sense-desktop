use framesense_raw::auto_correct_file_with_look;
use framesense_raw::pipeline::Look;
use std::path::Path;
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let src = Path::new(&args[0]);
    for (name, look) in [("natural", Look::Natural), ("warm", Look::Warm), ("golden", Look::Golden)] {
        let dst = std::env::temp_dir().join(format!("look-{}-{}.jpg", name, src.file_stem().unwrap().to_string_lossy()));
        match auto_correct_file_with_look(src, &dst, 88, look) {
            Ok((w, h)) => println!("{name}: {}x{} {}", w, h, dst.display()),
            Err(e) => println!("{name}: ERROR {e}"),
        }
    }
}
