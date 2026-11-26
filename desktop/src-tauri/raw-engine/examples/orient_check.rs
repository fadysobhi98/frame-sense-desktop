use rawler::imgop::develop::RawDevelop;
use std::path::Path;
fn main() {
    for arg in std::env::args().skip(1) {
        let path = Path::new(&arg);
        // 1. EXIF tag read directly from the file
        let exif_val = std::fs::File::open(path).ok().and_then(|f| {
            let mut r = std::io::BufReader::new(f);
            exif::Reader::new().read_from_container(&mut r).ok()
        }).and_then(|e| e.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
            .and_then(|f| f.value.get_uint(0)))
            .map(|v| v as u16);
        // 2. rawler orientation
        let raw = rawler::decode_file(path).expect("decode");
        println!("{arg}: exif_tag={:?} rawler={:?} raw_dims={}x{}", exif_val, raw.orientation, raw.width, raw.height);
        // 3. developed dims (does develop_intermediate rotate?)
        let dev = RawDevelop::default();
        let im = dev.develop_intermediate(&raw).expect("develop");
        let dims = im.to_dynamic_image().map(|d| (d.width(), d.height()));
        println!("   developed_dims={:?}", dims);
    }
}
