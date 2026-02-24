//! Comprehensive test suite for the `finger-core` cross-platform window automation API.
//!
//! Each test is self-contained: it finds (or launches) the editor window on its own.
//!
//! Usage:
//!   cargo run -p finger-test --bin test-autogui           # all tests
//!   cargo run -p finger-test --bin test-autogui 7 8 9     # specific tests by number
//!   cargo run -p finger-test --bin test-autogui -- --list  # list test names

use std::{fs, process::Command, thread, time::Duration};

use libtest_mimic::{Arguments, Failed, Trial, run};
use finger_core::{
    platform::{create_platform, WindowHandle},
    types::{Capture, CaptureRect},
};

// ─── platform constants ──────────────────────────────────────────────────────

const IS_DARWIN: bool = cfg!(target_os = "macos");
#[allow(dead_code)]
const IS_WIN32: bool = cfg!(target_os = "windows");

fn app_name() -> &'static str {
    if IS_DARWIN { "TextEdit" } else { "Notepad" }
}
fn select_all_key() -> &'static str {
    if IS_DARWIN { "cmd+a" } else { "ctrl+a" }
}
fn save_key() -> &'static str {
    if IS_DARWIN { "cmd+s" } else { "ctrl+s" }
}
fn window_pattern() -> &'static str {
    if IS_DARWIN { r"test\.txt" } else { r"test\.txt - Notepad" }
}
fn app_pattern() -> &'static str {
    if IS_DARWIN { "TextEdit" } else { "Notepad" }
}
fn window_title() -> &'static str {
    if IS_DARWIN { "test.txt" } else { "test.txt - Notepad" }
}

// ─── shared helpers ──────────────────────────────────────────────────────────

fn sleep(secs: f64) {
    thread::sleep(Duration::from_secs_f64(secs));
}

/// Save a raw BGRA `Capture` as a JPEG file.
fn save_screenshot_as_jpg(cap: &Capture, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut rgba = Vec::with_capacity(cap.data.len());
    for chunk in cap.data.chunks(4) {
        rgba.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]); // BGRA → RGBA
    }
    let img = image::RgbaImage::from_raw(cap.width, cap.height, rgba)
        .ok_or("buffer dimensions mismatch")?;
    img.save(path)?;
    Ok(())
}

/// Cmd/Ctrl+S, then read `test.txt` back.
fn save_and_read(win: &mut Box<dyn WindowHandle>) -> Option<String> {
    win.tap(save_key());
    sleep(0.3);
    if IS_DARWIN { win.tap("enter"); sleep(0.2); } // dismiss "Keep formatting?" sheet
    fs::read_to_string("test.txt").ok()
}

/// Sample up to `limit` unique RGB colours from a BGRA `Capture`.
fn count_unique_colors(cap: &Capture, limit: usize) -> usize {
    let mut seen = std::collections::HashSet::new();
    let step = ((cap.data.len() / 4) / 10_000).max(1);
    let mut i = 0usize;
    while i + 3 < cap.data.len() {
        seen.insert((cap.data[i+2], cap.data[i+1], cap.data[i]));
        if seen.len() >= limit { break; }
        i += step * 4;
    }
    seen.len()
}

// ─── per-test setup ──────────────────────────────────────────────────────────

/// Ensure the editor is open with `test.txt`. No-op if already visible.
fn ensure_editor() -> Result<(), Failed> {
    let platform = create_platform(false);
    if !platform.get_instances(window_pattern()).is_empty() {
        return Ok(());
    }
    if IS_DARWIN {
        let _ = Command::new("rm").args(["-f", "test.txt"]).status();
        let _ = Command::new("touch").arg("test.txt").status();
        Command::new("open").arg("test.txt").status()
            .map_err(|e| Failed::from(format!("open test.txt: {e}")))?;
    } else {
        let _ = Command::new("cmd").args(["/C", "del /f test.txt 2>nul"]).status();
        let _ = Command::new("cmd").args(["/C", "type nul > test.txt"]).status();
        Command::new("cmd").args(["/C", "start notepad test.txt"]).status()
            .map_err(|e| Failed::from(format!("start notepad: {e}")))?;
    }
    sleep(2.0);
    Ok(())
}

/// Obtain a ready, activated window handle. Called at the top of every test.
fn setup_window() -> Result<Box<dyn WindowHandle>, Failed> {
    let platform = create_platform(false);
    if !platform.get_instances("ThisWindowDoesNotExist12345").is_empty() {
        return Err(Failed::from("running in stub mode – unsupported platform"));
    }
    ensure_editor()?;
    let windows: Vec<_> = platform.get_instances(window_pattern())
        .into_iter()
        .chain(platform.get_instances(app_pattern()))
        .collect();
    let (id, _) = windows.into_iter().next()
        .ok_or_else(|| Failed::from(format!("{} window not found after launch", app_name())))?;
    let mut win = platform.create_window(window_title(), id);
    win.update();
    win.activate();
    sleep(0.3);
    Ok(win)
}

// ─── individual tests ────────────────────────────────────────────────────────

fn test_01_window_creation() -> Result<(), Failed> {
    let win = setup_window()?;
    win.region().ok_or_else(|| Failed::from("region is None after update"))?;
    println!("  ✓ window controller created and region valid");
    Ok(())
}

fn test_02_window_update() -> Result<(), Failed> {
    let mut win = setup_window()?;
    win.update();
    let r = win.region().ok_or_else(|| Failed::from("region is None"))?;
    if r.w <= 0 || r.h <= 0 {
        return Err(Failed::from(format!("invalid dimensions: {}x{}", r.w, r.h)));
    }
    if r.r <= r.l || r.b <= r.t {
        return Err(Failed::from("invalid bounds"));
    }
    println!("  region: {}x{} at ({}, {})", r.w, r.h, r.l, r.t);
    println!("  center: ({}, {})", r.cx, r.cy);
    Ok(())
}

fn test_03_window_activation() -> Result<(), Failed> {
    let mut win = setup_window()?;
    win.activate();
    sleep(0.5);
    println!("  ✓ activate() returned without error");
    Ok(())
}

fn test_04_keyboard_tap() -> Result<(), Failed> {
    let mut win = setup_window()?;
    win.tap(select_all_key()); win.tap("delete"); sleep(0.2);
    println!("  tap a, b, left, c → expect \"acb\"");
    win.tap("a"); sleep(0.1);
    win.tap("b"); sleep(0.1);
    win.tap("left"); sleep(0.1);
    win.tap("c"); sleep(0.3);
    let got = save_and_read(&mut win).ok_or_else(|| Failed::from("could not read test.txt"))?;
    if got != "acb" {
        return Err(Failed::from(format!("expected \"acb\", got {:?}", got)));
    }
    Ok(())
}

fn test_05_keyboard_type() -> Result<(), Failed> {
    let mut win = setup_window()?;
    win.tap(select_all_key()); win.tap("delete"); sleep(0.3);
    let text = "Hello World 123";
    println!("  type_text({text:?})");
    win.type_text(text); sleep(0.3);
    let got = save_and_read(&mut win).ok_or_else(|| Failed::from("could not read test.txt"))?;
    if got != text {
        return Err(Failed::from(format!("expected {text:?}, got {got:?}")));
    }
    Ok(())
}

fn test_06_keyboard_send() -> Result<(), Failed> {
    let mut win = setup_window()?;
    win.tap(select_all_key()); win.tap("delete"); sleep(0.3);
    let text = "Test Command";
    println!("  type_text({text:?}) + enter");
    win.type_text(text); win.tap("enter"); sleep(0.3);
    let raw = save_and_read(&mut win).ok_or_else(|| Failed::from("could not read test.txt"))?;
    let norm = raw.replace("\r\n", "\n").replace('\r', "\n");
    if norm != format!("{text}\n") && norm != text {
        return Err(Failed::from(format!(
            "expected {text:?} + newline, got {:?}",
            raw.replace('\n', "\\n").replace('\r', "\\r")
        )));
    }
    Ok(())
}

fn test_07_keyboard_special_keys() -> Result<(), Failed> {
    let mut win = setup_window()?;
    win.tap(select_all_key()); win.tap("delete"); sleep(0.3);
    println!("  a→space→b→enter→c→d→left→up→space→down→right→f");
    for (key, delay) in &[
        ("a", 0.05), ("space", 0.05), ("b", 0.05), ("enter", 0.05),
        ("c", 0.05), ("d", 0.05), ("left", 0.05), ("up", 0.05),
        ("space", 0.05), ("down", 0.05), ("right", 0.05), ("f", 0.3),
    ] { win.tap(key); sleep(*delay); }
    let raw = save_and_read(&mut win).ok_or_else(|| Failed::from("could not read test.txt"))?;
    let norm = raw.replace("\r\n", "\n").replace('\r', "\n");
    // Accept autocapitalize (A vs a) and single/double space (macOS cursor math)
    let valid = ["a b\ncdf", "a  b\ncdf", "A b\ncdf", "A  b\ncdf"];
    if !valid.contains(&norm.as_str()) {
        return Err(Failed::from(format!(
            "expected one of {valid:?}, got {:?}",
            raw.replace('\n', "\\n").replace('\r', "\\r")
        )));
    }
    Ok(())
}

fn test_08_mouse_click() -> Result<(), Failed> {
    let mut win = setup_window()?;
    win.tap(select_all_key()); win.tap("delete"); sleep(0.3);
    let region = win.region().ok_or_else(|| Failed::from("region unavailable"))?;
    win.type_text("Click Test"); sleep(0.3);
    // Move left 5 → cursor before space: "Click| Test"
    for _ in 0..5 { win.tap("left"); sleep(0.05); }
    sleep(0.2);
    // Click window center: single short line → cursor moves past end
    let (rx, ry) = (0.5, 0.5);
    println!("  click_relative({rx}, {ry}) → ({}, {}) screen px",
        region.l + (rx * region.w as f64) as i32,
        region.t + (ry * region.h as f64) as i32);
    win.click_relative(rx, ry); sleep(0.2);
    win.type_text(" Works"); sleep(0.3);
    let got = save_and_read(&mut win).ok_or_else(|| Failed::from("could not read test.txt"))?;
    if got != "Click Test Works" {
        return Err(Failed::from(format!("expected \"Click Test Works\", got {got:?}")));
    }
    Ok(())
}

fn test_09_mouse_click_relative() -> Result<(), Failed> {
    let mut win = setup_window()?;
    win.tap(select_all_key()); win.tap("delete"); sleep(0.3);
    win.type_text("Relative Test"); sleep(0.3);
    // Move left 5 → cursor before space: "Relative| Test"
    for _ in 0..5 { win.tap("left"); sleep(0.05); }
    sleep(0.2);
    // Click window center → cursor past end of text
    println!("  click_relative(0.5, 0.5) — window center");
    win.click_relative(0.5, 0.5); sleep(0.2);
    win.type_text(" Works"); sleep(0.3);
    let got = save_and_read(&mut win).ok_or_else(|| Failed::from("could not read test.txt"))?;
    if got != "Relative Test Works" {
        return Err(Failed::from(format!("expected \"Relative Test Works\", got {got:?}")));
    }
    Ok(())
}

fn test_10_capture_full() -> Result<(), Failed> {
    let mut win = setup_window()?;
    let cap = win.capture(None).ok_or_else(|| Failed::from(
        "capture() returned None – check Screen Recording permission"
    ))?;
    if cap.width == 0 || cap.height == 0 {
        return Err(Failed::from(format!("invalid dimensions: {}x{}", cap.width, cap.height)));
    }
    let expected_bytes = (cap.width * cap.height * 4) as usize;
    if cap.data.len() != expected_bytes {
        return Err(Failed::from(format!("data size mismatch: {} != {}", cap.data.len(), expected_bytes)));
    }
    let unique = count_unique_colors(&cap, 50);
    if unique < 3 {
        return Err(Failed::from(format!("buffer looks corrupt ({unique} unique colours)")));
    }
    match save_screenshot_as_jpg(&cap, "test-capture-full.jpg") {
        Ok(_)  => println!("  ✓ saved test-capture-full.jpg"),
        Err(e) => println!("  ⚠️  could not save JPEG: {e}"),
    }
    println!("  {}x{}, {unique}+ colours, {} bytes", cap.width, cap.height, cap.data.len());
    Ok(())
}

fn test_11_capture_partial() -> Result<(), Failed> {
    let mut win = setup_window()?;
    win.update();
    let region = win.region().ok_or_else(|| Failed::from("region unavailable"))?;
    let rect = CaptureRect { l: 10, t: 10, w: region.w * 3 / 10, h: region.h * 3 / 10 };
    println!("  window {}x{} → capturing {}x{} at ({}, {})",
        region.w, region.h, rect.w, rect.h, rect.l, rect.t);
    let cap = win.capture(Some(rect)).ok_or_else(|| Failed::from("capture(partial) returned None"))?;
    // macOS aligns bpr to 128-byte boundaries, so cap.width >= rect.w
    if (cap.width as i32) < rect.w || cap.height != rect.h as u32 {
        return Err(Failed::from(format!(
            "dimension mismatch: expected ≥{}x{}, got {}x{}",
            rect.w, rect.h, cap.width, cap.height
        )));
    }
    let expected_bytes = (cap.width * cap.height * 4) as usize;
    if cap.data.len() != expected_bytes {
        return Err(Failed::from(format!("data size mismatch: {} != {}", cap.data.len(), expected_bytes)));
    }
    let unique = count_unique_colors(&cap, 50);
    if unique < 3 {
        let _ = save_screenshot_as_jpg(&cap, "test-capture-partial-debug.jpg");
        return Err(Failed::from(format!("buffer looks corrupt ({unique} unique colours)")));
    }
    match save_screenshot_as_jpg(&cap, "test-capture-partial.jpg") {
        Ok(_)  => println!("  ✓ saved test-capture-partial.jpg"),
        Err(e) => println!("  ⚠️  could not save JPEG: {e}"),
    }
    println!("  {}x{}, {unique}+ colours", cap.width, cap.height);
    Ok(())
}

// ─── entry point ─────────────────────────────────────────────────────────────

fn main() {
    let mut args = Arguments::from_args();
    // GUI tests must run serially — they all interact with the same OS window.
    args.test_threads = Some(1);

    let tests = vec![
        Trial::test("01_window_creation",      || test_01_window_creation()),
        Trial::test("02_window_update",         || test_02_window_update()),
        Trial::test("03_window_activation",     || test_03_window_activation()),
        Trial::test("04_keyboard_tap",          || test_04_keyboard_tap()),
        Trial::test("05_keyboard_type",         || test_05_keyboard_type()),
        Trial::test("06_keyboard_send",         || test_06_keyboard_send()),
        Trial::test("07_keyboard_special_keys", || test_07_keyboard_special_keys()),
        Trial::test("08_mouse_click",           || test_08_mouse_click()),
        Trial::test("09_mouse_click_relative",  || test_09_mouse_click_relative()),
        Trial::test("10_capture_full",          || test_10_capture_full()),
        Trial::test("11_capture_partial",       || test_11_capture_partial()),
    ];

    run(&args, tests).exit();
}
