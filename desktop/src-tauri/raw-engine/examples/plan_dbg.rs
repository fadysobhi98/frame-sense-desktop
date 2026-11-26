fn main(){
    let a:Vec<String>=std::env::args().collect();
    let img=image::open(&a[1]).unwrap().to_rgb32f();
    let mut rgb=img.clone();
    framesense_raw::denoise::denoise(&mut rgb);
    framesense_raw::pipeline::local_tone_map(&mut rgb,0.5);
    let plan=framesense_raw::pipeline::plan_for_image(&rgb,framesense_raw::pipeline::Look::Warm);
    println!("{:?}",plan);
}
