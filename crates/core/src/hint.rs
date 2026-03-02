use crate::types::Capture;

/// Save capture data for debugging.
/// Writes decoded nibbles as hex, raw RGB values, and a PNG to logs/.
/// Only compiled when the `debug-capture` feature is enabled.
fn save_capture(capture: &Capture) {
    use std::fmt::Write as _;
    use std::fs;
    use std::io::Write as _;

    let stride = capture.bytes_per_row;
    let mut raw_hex = String::new();
    let mut rgb_data = String::new();

    for y in 0..capture.height {
        for x in 0..capture.width {
            let nibble = get_nibble(capture, x, y);
            if x > 0 {
                raw_hex.push(' ');
            }
            write!(raw_hex, "{:02x}", nibble).ok();

            let idx = (y * stride + x * 4) as usize;
            let b = capture.data[idx];
            let g = capture.data[idx + 1];
            let r = capture.data[idx + 2];
            if x > 0 {
                rgb_data.push_str(" | ");
            }
            write!(rgb_data, "{:3},{:3},{:3}", r, g, b).ok();
        }
        raw_hex.push('\n');
        rgb_data.push('\n');
    }

    // Resolve logs/ relative to workspace root (two levels up from this crate)
    let logs_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../logs");
    fs::create_dir_all(&logs_dir).ok();
    if let Ok(mut f) = fs::File::create(logs_dir.join("hint-v2-raw.txt")) {
        f.write_all(raw_hex.as_bytes()).ok();
    }
    if let Ok(mut f) = fs::File::create(logs_dir.join("hint-v2-rgb.txt")) {
        f.write_all(rgb_data.as_bytes()).ok();
    }

    // Save PNG: convert BGRA → RGBA
    {
        let mut rgba = Vec::with_capacity(capture.data.len());
        for chunk in capture.data.chunks(4) {
            rgba.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]);
        }
        if let Some(img) = image::RgbaImage::from_raw(capture.width, capture.height, rgba) {
            img.save(logs_dir.join("hint-v2-capture.png")).ok();
        }
    }

    crate::logger::info_p(
        "hint",
        &format!(
            "saved capture {}x{} to logs/",
            capture.width, capture.height
        ),
    );
}

/// Extract a 7-bit value from a single pixel in a Capture buffer.
/// Encoding: G[6:4] << 4 | R[6:5] << 2 | B[6:5]
/// Capture is always BGRA byte order.
fn get_nibble(capture: &Capture, x: u32, y: u32) -> u8 {
    let idx = (y * capture.bytes_per_row + x * 4) as usize;
    let b = capture.data[idx];
    let g = capture.data[idx + 1];
    let r = capture.data[idx + 2];

    let r_bit = (r >> 6) & 1;
    let g_2bits = (g >> 5) & 3;
    let b_bit = (b >> 6) & 1;

    (r_bit << 3) | (g_2bits << 1) | b_bit
}

/// Decode the hint-v2 color grid from a capture.
/// return segments of strings
pub fn decode_hint_v2(capture: &Capture) -> Option<Vec<String>> {
    // save_capture(capture);

    let mut nibbles: Vec<u8> = Vec::new();
    let mut rid: u8 = 1;

    for y_start in (0..capture.height).step_by(3) {
        // try_decode_row_fsm only responds to the specific rid marker
        if let Some(decoded) = try_decode_row_fsm(capture, y_start, rid) {
            // RLE normalize: 3 marker blocks total → each block = block_width/3 px
            nibbles.extend_from_slice(&decoded);
            rid += 1; // only advance when this row was found
        }
    }

    // println!("Decoded nibbles: {:?}", nibbles);
    // pair nibbles big-endian into bytes
    let mut all_bytes: Vec<u8> = Vec::new();
    let mut i = 0;
    while i + 1 < nibbles.len() {
        all_bytes.push((nibbles[i] << 4) | nibbles[i + 1]);
        i += 2;
    }

    if all_bytes.is_empty() {
        return None;
    }

    let raw = String::from_utf8_lossy(&all_bytes).into_owned();
    let mut result = vec![raw];
    for seg in all_bytes.split(|&b| b == b',') {
        result.push(String::from_utf8_lossy(seg).into_owned());
    }
    Some(result)
}

/// FSM that extracts RLE-encoded bytes from a row.
/// Returns only 4-bit data, no markers
fn try_decode_row_fsm(capture: &Capture, y: u32, rid: u8) -> Option<Vec<u8>> {
    #[derive(Debug, PartialEq, PartialOrd)]
    enum State {
        Start,
        M0,     // accumulating 0xf marker bytes
        M1,     // accumulating rid >> 4
        M2,     // accumulating rid & 0xf
        Decode, // accumulating data bytes
        End1,   // found trailing 0 0 f marker
        Done,
    }

    let mut state = State::Start;
    let mut block_width: i32 = 0;
    let mut decoded: Vec<u8> = Vec::new();
    let mut acc: i32 = 0;
    let mut last: u8 = 0;

    let max_x = capture.width;

    // println!("row {y}: search {rid}");
    for x in (0..max_x).step_by(1) {
        if x * 4 + 3 >= capture.bytes_per_row {
            break;
        }
        let val = get_nibble(capture, x, y);

        // Marker must start within first 100 pixels; bail early if not found
        if x >= 50 && state < State::Decode {
            return None;
        }

        match state {
            State::Start => {
                if val == 0xf {
                    state = State::M0;
                    block_width = 1;
                }
            }
            State::M0 => {
                if val == 0xf {
                    block_width += 1;
                } else if val == rid && block_width > 2 {
                    state = State::M1;
                    block_width += 1;
                } else {
                    state = State::Start;
                }
            }
            State::M1 => {
                if val == rid {
                    block_width += 1;
                } else if val == 0 && block_width > 4 {
                    state = State::M2;
                    block_width += 1;
                } else {
                    state = State::Start;
                }
            }
            State::M2 => {
                if val == 0 {
                    block_width += 1;
                } else {
                    state = State::Decode;
                    decoded.clear();
                    last = val;
                    acc = 1;
                    block_width /= 6; // half block width
                }
            }
            State::Decode => {
                if val == last {
                    acc += 1;
                    if acc > block_width {
                        if val == 0x0 && decoded.len() & 1 == 0 {
                            state = State::End1;
                        } else {
                            decoded.push(val);
                            acc = -block_width;
                        }
                    }
                } else {
                    last = val;
                    acc = 1;
                }
            }
            State::End1 => {
                if val == 0xf {
                    state = State::Done;
                    break;
                }
                // Stay in End1, absorb trailing marker bytes
            }
            State::Done => break,
        }
    }

    if state != State::Done || decoded.is_empty() || block_width == 0 {
        return None;
    }

    Some(decoded)
}
