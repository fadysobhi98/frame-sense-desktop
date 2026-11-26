fn main(){
    let a: Vec<String> = std::env::args().collect();
    let mut img = image::open(&a[1]).unwrap().to_rgb8();
    let r = framesense_raw::retouch::retouch_image(&mut img, std::path::Path::new("/tmp/landmark-proto/libonnxruntime.so"), std::path::Path::new("/tmp/models_eye"));
    println!("{} -> {:?} ({}x{})", a[1], r, img.width(), img.height());
}
