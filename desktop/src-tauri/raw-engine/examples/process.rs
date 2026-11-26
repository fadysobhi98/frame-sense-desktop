fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: process <src> <dst.jpg>");
        std::process::exit(2);
    }
    let t = std::time::Instant::now();
    match framesense_raw::auto_correct_file(
        std::path::Path::new(&args[1]),
        std::path::Path::new(&args[2]),
        90,
    ) {
        Ok((w, h)) => println!("OK {}x{} in {:.2?}", w, h, t.elapsed()),
        Err(e) => {
            eprintln!("FAIL {e}");
            std::process::exit(1);
        }
    }
}
