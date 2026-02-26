//! Decode hint-v2 from an arbitrary PNG/image file.
//!
//! Usage:
//!   cargo run -p finger-test --bin decode -- path/to/image.png

use finger_core::{hint::decode_hint_v2, types::Capture};

fn load_capture(path: &str) -> Result<Capture, Box<dyn std::error::Error>> {
    let img = image::open(path)?.into_rgba8();
    let (width, height) = img.dimensions();
    let raw = img.into_raw(); // RGBA bytes (R,G,B,A per pixel)

    // Reverse the BGRA→RGBA swap done at save time: [R,G,B,A] → [B,G,R,A]
    let mut bgra = Vec::with_capacity(raw.len());
    for chunk in raw.chunks(4) {
        bgra.push(chunk[2]); // B ← saved R
        bgra.push(chunk[1]); // G ← saved G
        bgra.push(chunk[0]); // R ← saved B
        bgra.push(chunk[3]); // A
    }

    let bytes_per_row = width * 4;
    Ok(Capture { data: bgra, width, height, bytes_per_row })
}

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("Usage: decode <image-path>");
            std::process::exit(1);
        }
    };

    println!("Loading: {path}");

    let capture = match load_capture(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to load {path}: {e}");
            std::process::exit(1);
        }
    };

    println!("Capture: {}x{}, {} bytes/row", capture.width, capture.height, capture.bytes_per_row);

    match decode_hint_v2(&capture) {
        Some(table) => {
            println!("Decoded ({} entries):", table.len());
            for (i, seg) in table.iter().enumerate() {
                println!("  [{i}] = {seg:?}");
            }
        }
        None => eprintln!("decode_hint_v2 returned None — no valid hint found"),
    }
}
