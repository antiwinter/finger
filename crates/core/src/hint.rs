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

    let r_bits = (r >> 5) & 0x03;
    let g_bits = (g >> 4) & 0x07;
    let b_bits = (b >> 5) & 0x03;

    (g_bits << 4) | (r_bits << 2) | b_bits
}

#[derive(Debug, Clone)]
struct DecodedChar {
    c: u8,
    n: u32, // number of consecutive pixels with this value
}

/// Decode the hint-v2 color grid from a capture.
/// Scans every 3rd pixel on each row, uses an FSM to detect
/// the marker sequence [0x00...][0x7F...] data [0x7F...][0x00...]
/// Splits decoded bytes by `~` into segments.
/// Segments containing non-alphanumeric bytes have all bytes OR'd with 0x80
/// and are re-decoded as UTF-8 (lossy).
/// Returns Vec where index 0 = raw string, indices 1.. = parsed segments.
pub fn decode_hint_v2(capture: &Capture) -> Option<Vec<String>> {
    save_capture(capture);

    for y_start in (0..capture.height).step_by(3) {
        if let Some(raw_bytes) = try_decode_row_raw(capture, y_start) {
            if raw_bytes.is_empty() {
                continue;
            }
            let raw_string: String = raw_bytes
                .iter()
                .filter(|&&b| b == b'~' || (b as char).is_ascii_graphic() || b == b' ')
                .map(|&b| b as char)
                .collect();

            let mut result = vec![raw_string];
            for seg in raw_bytes.split(|&b| b == b'~') {
                let is_alnum = seg.iter().all(|&b| b.is_ascii_alphanumeric());
                if is_alnum {
                    result.push(String::from_utf8_lossy(seg).into_owned());
                } else {
                    let utf8_bytes: Vec<u8> = seg.iter().map(|&b| b | 0x80).collect();
                    result.push(String::from_utf8_lossy(&utf8_bytes).into_owned());
                }
            }
            return Some(result);
        }
    }
    None
}

/// Try to decode raw bytes from a single row (FSM + RLE normalization).
/// Returns all decoded bytes without ASCII filtering.
fn try_decode_row_raw(capture: &Capture, y: u32) -> Option<Vec<u8>> {
    let (decoded, marker_width) = try_decode_row_fsm(capture, y)?;

    let mut result = Vec::new();
    for d in &decoded {
        let char_count = ((d.n as f64 * 2.0) / marker_width as f64).round() as u32;
        for _ in 0..char_count.max(1) {
            result.push(d.c);
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// FSM that extracts RLE-encoded bytes from a row.
/// Returns (decoded_chars, marker_width) or None.
fn try_decode_row_fsm(capture: &Capture, y: u32) -> Option<(Vec<DecodedChar>, u32)> {
    #[derive(Debug, PartialEq, PartialOrd)]
    enum State {
        Start,
        M0,     // accumulating 0x00 marker bytes
        M1,     // accumulating 0x7F marker bytes
        Decode, // accumulating data bytes
        End1,   // found trailing 0x7F marker
        Done,
    }

    let mut state = State::Start;
    let mut marker_width: u32 = 0;
    let mut decoded: Vec<DecodedChar> = Vec::new();

    let max_x = capture.width;

    for x in (0..max_x).step_by(1) {
        if x * 4 + 3 >= capture.bytes_per_row {
            break;
        }
        let val = get_nibble(capture, x, y);

        // Marker must start within first 100 pixels; bail early if not found
        if x >= 100 && state < State::Decode {
            return None;
        }

        match state {
            State::Start => {
                if val == 0x00 {
                    state = State::M0;
                    marker_width = 1;
                }
            }
            State::M0 => {
                if val == 0x00 {
                    marker_width += 1;
                } else if val == 0x7F {
                    state = State::M1;
                    marker_width += 1;
                } else {
                    state = State::Start;
                }
            }
            State::M1 => {
                if val == 0x7F {
                    marker_width += 1;
                } else {
                    state = State::Decode;
                    decoded.push(DecodedChar { c: val, n: 1 });
                }
            }
            State::Decode => {
                if marker_width < 5 {
                    // Invalid marker width, restart
                    state = State::Start;
                    marker_width = 0;
                    decoded.clear();
                } else if val == 0x7F {
                    state = State::End1;
                } else if let Some(last) = decoded.last_mut() {
                    if last.c == val {
                        last.n += 1;
                    } else {
                        decoded.push(DecodedChar { c: val, n: 1 });
                    }
                }
            }
            State::End1 => {
                if val == 0x00 {
                    state = State::Done;
                    break;
                }
                // Stay in End1, absorb trailing marker bytes
            }
            State::Done => break,
        }
    }

    if state != State::Done || decoded.is_empty() || marker_width == 0 {
        return None;
    }

    Some((decoded, marker_width))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_nibble_zero() {
        // All zeros should decode to 0
        let capture = Capture {
            data: vec![0, 0, 0, 255], // BGRA
            width: 1,
            height: 1,
            bytes_per_row: 4,
        };
        assert_eq!(get_nibble(&capture, 0, 0), 0);
    }
}
