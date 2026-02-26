//! Find the WoW window, activate it, wait 1s, capture and decode the hint.
//!
//! Usage:
//!   cargo run -p finger-test --bin wow-decode

use finger_core::{
    hint::decode_hint_v2,
    platform::create_platform,
    types::CaptureRect,
};
use std::{thread, time::Duration};

fn main() {
    let platform = create_platform(false);

    let instances = platform.get_instances(r"魔兽世界|wow|world of warcraft");
    if instances.is_empty() {
        eprintln!("No WoW window found");
        std::process::exit(1);
    }

    println!("Found {} window(s):", instances.len());
    for (id, title) in &instances {
        println!("  [{id}] {title}");
    }

    let (id, title) = &instances[0];
    println!("\nUsing: [{id}] {title}");

    let mut window = platform.create_window(title, *id);
    window.activate();
    println!("Activated, waiting 1s...");
    thread::sleep(Duration::from_secs(1));

    window.update();
    let region = window.region();
    println!("Window region: {:?}", region);

    let capture = match window.capture(Some(CaptureRect { l: 0, t: 0, w: 320, h: 80 })) {
        Some(c) => c,
        None => {
            eprintln!("Capture failed");
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
        None => eprintln!("decode_hint_v2 returned None — no valid hint found in capture"),
    }
}
