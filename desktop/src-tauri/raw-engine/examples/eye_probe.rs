use std::path::Path;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let r = framesense_raw::photo_has_closed_eyes(Path::new(&a[1]), Path::new(&a[2]), Path::new(&a[3]));
    println!("{} closed={:?}", a[1], r);
}
