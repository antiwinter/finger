//! Decode hint-v2 from a captured JPEG.
//!
//! The JPEG was saved by `save_screenshot_as_jpg` which converts BGRA→RGBA.
//! We reverse that (swap R↔B per pixel) to reconstruct the BGRA Capture that
//! `decode_hint_v2` expects.
//!
//! Usage:
//!   cargo run -p finger-test --bin test-hint                      # uses bundled JPEG
//!   cargo run -p finger-test --bin test-hint -- path/to/file.jpg  # custom file

use finger_core::{hint::decode_hint_v2, types::Capture};

fn load_capture_from_jpeg(path: &str) -> Result<Capture, Box<dyn std::error::Error>> {
    let img = image::open(path)?.into_rgba8();
    let (width, height) = img.dimensions();
    let raw = img.into_raw(); // RGBA bytes (R,G,B,A per pixel)

    // Reverse the BGRA→RGBA swap done at save time: [R,G,B,A] → [B,G,R,A]
    let mut bgra = Vec::with_capacity(raw.len());
    for chunk in raw.chunks(4) {
        bgra.push(chunk[2]); // B = saved R
        bgra.push(chunk[1]); // G = saved G
        bgra.push(chunk[0]); // R = saved B
        bgra.push(chunk[3]); // A
    }

    let bytes_per_row = width * 4;
    Ok(Capture { data: bgra, width, height, bytes_per_row })
}

fn main() {
    let path = std::env::args().nth(1)
        .unwrap_or_else(|| "crates/test/hint-v2-capture.png".to_string());

    println!("Loading: {path}");

    let capture = match load_capture_from_jpeg(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to load {path}: {e}");
            std::process::exit(1);
        }
    };

    println!("Capture: {}x{}, {} bytes", capture.width, capture.height, capture.data.len());

    match decode_hint_v2(&capture) {
        Some(hint) => println!("Decoded: {hint:?}"),
        None => {
            eprintln!("error: decode_hint_v2 returned None");
            std::process::exit(1);
        }
    }
}
