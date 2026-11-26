use std::path::Path;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let r = framesense_raw::auto_correct_file_v2(Path::new(&a[1]), Path::new(&a[3]), 90, framesense_raw::pipeline::Look::Warm, 0, None, Some(Path::new(&a[2])));
    println!("{:?}", r);
}
