use crate::types::Capture;

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
/// Returns the decoded ASCII string, or None.
pub fn decode_hint_v2(capture: &Capture) -> Option<String> {
    // Try multiple Y rows (every 3rd row) to find the hint strip
    for y_start in (0..capture.height.min(60)).step_by(3) {
        if let Some(s) = try_decode_row(capture, y_start) {
            return Some(s);
        }
    }
    None
}

fn try_decode_row(capture: &Capture, y: u32) -> Option<String> {
    #[derive(Debug, PartialEq)]
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

    let max_x = capture.width.min(200);

    for x in (0..max_x).step_by(1) {
        if x * 4 + 3 >= capture.bytes_per_row {
            break;
        }
        let val = get_nibble(capture, x, y);

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
                if val == 0x7F {
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

    // Normalize: each character spans approximately marker_width pixels
    let mut result = String::new();
    for d in &decoded {
        let char_count = ((d.n as f64 * 2.0) / marker_width as f64).round() as u32;
        let ch = d.c as char;
        if ch.is_ascii_graphic() || ch == ' ' {
            for _ in 0..char_count.max(1) {
                result.push(ch);
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
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
