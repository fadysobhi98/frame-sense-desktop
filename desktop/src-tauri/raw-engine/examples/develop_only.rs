fn main() {
    let args: Vec<String> = std::env::args().collect();
    let raw = rawler::decode_file(std::path::Path::new(&args[1])).unwrap();
    let dev = rawler::imgop::develop::RawDevelop::default();
    let img = dev.develop_intermediate(&raw).unwrap().to_dynamic_image().unwrap();
    let rgb8 = img.to_rgb8();
    use image::ImageEncoder;
    let mut out = std::io::BufWriter::new(std::fs::File::create(&args[2]).unwrap());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
        .write_image(&rgb8, img.width(), img.height(), image::ExtendedColorType::Rgb8).unwrap();
    println!("OK");
}
