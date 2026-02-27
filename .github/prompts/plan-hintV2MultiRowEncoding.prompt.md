# Plan: Multi-row 4-bit-per-block Hint v2

Replace the current single-row 7-bit encoding with a multi-row 4-bit-per-block scheme. Each block encodes 1 nibble (R=1 bit, G=2 bits, B=1 bit). Two blocks = one full byte (8-bit clean, no Chinese heuristic). Text is split into rows; each row is bracketed by a marker pair encoding the row index. The Rust decoder is rewritten to collect all rows in sequence.

---

## Encoding formulas (one block = 1 nibble)

Given a nibble `n` (0–15):
- `r_bit   = (n >> 3) & 1`
- `g_2bits = (n >> 1) & 3`
- `b_bit   =  n       & 1`

Channel values (+offset centers the value in its quantization bin, making rounding unambiguous against color correction):
- `r = 0x80 + r_bit   * 64 + 32`  → 160 (0xA0) or 224 (0xE0), ±32 margin
- `g = 0x80 + g_2bits * 32 + 16`  → 144/176/208/240, ±16 margin
- `b = 0x80 + b_bit   * 64 + 32`  → 160 (0xA0) or 224 (0xE0), ±32 margin

Decoding (shift directly — the +offset is already inside the encoded value, no subtraction needed):
- `r_bit   = (r >> 6) & 1`
- `g_2bits = (g >> 5) & 3`
- `b_bit   = (b >> 6) & 1`

Nibble value: `(r_bit << 3) | (g_2bits << 1) | b_bit`

---

## Lua encoder — hint-v2.lua

1. **Rename/rewrite `renderBlock`** → `renderNibble(nibble)` using the 4-bit formulas above.
   - Takes nibble 0–15, sets R/G/B via the formulas.
   - Positions block with both `xOffset = (blockIndex-1) * blockSize` and `yOffset` (passed in or stored as `self.rowOffset`).
   - Increments `self.blockIndex`.

2. **Add `renderByte(byte)`**: big-endian nibble order — high nibble first, low nibble second.
   - Calls `renderNibble(bit.rshift(byte, 4))` then `renderNibble(bit.band(byte, 0x0F))`.
   - Full 8-bit data, no bit-7 masking.
   - Example: `0x30` → blocks `3`, `0`; `0x31` → blocks `3`, `1`.

3. **Update `ShowSingle`** → becomes multi-row:
   - `bytesPerRow = math.floor((self.maxBlocks - 6) / 2)` (= 22 with maxBlocks=50; 6 blocks = 3-block start marker + 3-block end marker).
   - Convert `text` to a byte table via `string.byte(text, 1, #text)`.
   - Split byte table into chunks of `bytesPerRow` bytes.
   - For each chunk at row index `rowIdx` (0-based):
     - Set `self.blockIndex = 1` and `self.rowOffset = rowIdx * self.blockSize`.
     - Render **start marker**: `renderNibble(0xF)`, `renderNibble(0x0)`, `renderNibble(rowIdx)` → 3 blocks.
     - Render each data byte with `renderByte`.
     - Render **end marker**: `renderNibble(0x0)`, `renderNibble(0x0)`, `renderNibble(0xF)` → 3 blocks (fixed, same for all rows).
     - Track `maxBlockIndex` across rows for frame width.
   - After all rows, hide unused blocks (up to `maxBlocks * numRows`).
   - Show used blocks for all rows.
   - Set frame width = `maxBlockIndex * blockSize`, height = `numRows * blockSize`.

4. **Update `getBlock`**: block index is now global across rows, so `getBlock` uses a flat index. Or keep per-row blockIndex and offset block names by `rowIdx * maxBlocks`.

5. **`Show(hint)`** and **`Hide()`** unchanged.

---

## Rust decoder — crates/core/src/hint.rs

1. **`get_nibble(capture, x, y)`**: replace 7-bit formula with 4-bit decoding:
   ```rust
   let r_bit   = (r >> 6) & 1;
   let g_2bits = (g >> 5) & 3;
   let b_bit   = (b >> 6) & 1;
   (r_bit << 3) | (g_2bits << 1) | b_bit
   ```
   No `saturating_sub` needed — the +offset offsets are baked into the encoded channel values, so the threshold is naturally at the bit boundary after shifting.

2. **Add `get_byte(capture, x, y)`**: big-endian — reads hi nibble at `x`, lo nibble at `x+1`, returns `(hi << 4) | lo`.

3. **Rewrite `try_decode_row_fsm()`**:

   Start marker for row N: three nibble blocks `F`, `0`, `N`.
   End marker (all rows): three nibble blocks `0`, `0`, `F`, always at a byte (hi-nibble) boundary.

   Example — encoding `'0' '1'` (0x30 0x31) on row 0:
   ```
   F  0  0  |  3  0  3  1  |  0  0  F
   start    |  data bytes  |  end
   ```

   **Why `0 0 F` at a byte boundary is unambiguous**: all encoded data bytes are ≥ `0x10`
   (bytes `0x00`–`0x0F` are non-printable and never transmitted). Therefore the high nibble
   of any data byte is always ≥ `0x1`. A `0x0` at a hi-nibble position can only be the end
   marker, never data.

   FSM states (each accumulates an RLE run of the expected nibble value):

   | State | Expects | Records | On mismatch |
   |-------|---------|---------|-------------|
   | **Start** | `0xF` | `m0_px` | stay in Start |
   | **M1** | `0x0` | `m1_px` | → Start |
   | **M2** | any (row index) | `row_index`, `m2_px`; `marker_block_px = avg(m0,m1,m2 px)`; set `hi = true` | → Start |
   | **Decode** | any; if `hi && nibble == 0x0` → End1; else toggle `hi` | RLE nibble pairs | — |
   | **End1** | `0x0` run → End2 | — | → Decode (lo nibble `0x0` is valid; emit as data, set `hi = true`) |
   | **End2** | `0xF` → Done | — | → Decode (emit `0x00` byte, continue with `hi = true`) |
   | **Done** | — | — | — |

   After Done: return `(decoded_nibbles: Vec<DecodedNibble>, row_index: u8, marker_block_px: u32)`.

4. **Update `try_decode_row_raw()`** (the aggregating part — differs from original):
   - RLE normalization: for each `DecodedNibble { val, px }`, compute `count = round(px / marker_block_px)`.max(1).
   - Emit `count` copies of that nibble value.
   - Pair consecutive nibbles big-endian: first nibble is high, second is low → `byte = (hi << 4) | lo`.
   - Odd trailing nibble is discarded.

5. **Rewrite `decode_hint_v2()`**:
   - Remove the Chinese OR-with-0x80 heuristic entirely.
   - Scan all rows (by `y_start` stepping). For each successfully decoded row, record `(row_index, decoded_bytes)`.
   - After scanning all rows, sort decoded rows by `row_index`; concatenate bytes to form full text.
   - Split by `~` (0x7E) into segments; decode each segment as UTF-8 lossy directly (full 8-bit, no heuristic).
   - Return `Vec<String>` where index 0 = raw concatenated string, indices 1.. = `~`-segments.

6. **`save_capture`**: no changes needed.

7. **Update unit test `test_get_nibble_zero`**: with all-zero BGRA, channels are all 0, saturating subs give 0 bits → nibble = 0. Test still passes; update comment to reflect new formula.

---

## Decisions

- **Bit layout R=1, G=2, B=1**: G gets 2 bits because green is most perceptually sensitive; its 32-unit step (±16 margin) is narrower than R/B's 64-unit step (±32 margin), which is acceptable given macOS color correction is smallest in the green channel.
- **+offset encodes the bin center**: the +32/+16/+32 offsets are not "buffers" to subtract — they push each quantization level to the center of its decoding bin, so `>> N` thresholds are naturally correct without any subtraction.
- **Start marker `F 0 N`**: `F` is a rare leading nibble in text data; `0` second position confirms it; third nibble is row index 0–15, supporting up to 16 rows. Total overhead: 6 blocks per row (3 start + 3 end).
- **End marker `0 0 F` at byte boundary**: fixed across all rows. Safe because all encoded data bytes are ≥ `0x10`, so hi-nibble `0x0` is data-impossible. The FSM tracks byte parity (hi/lo) and only enters End1 at a hi-nibble boundary, making detection zero-false-positive.
- **No Chinese heuristic**: 2 blocks per byte gives full 8-bit data; UTF-8 multibyte sequences decode verbatim.

---

## Verification steps

1. **Lua**: trigger `LibHint:Show("some string longer than 21 chars, like this test")` in-game; screenshot; verify two rows of color blocks appear.
2. **Rust decode**: run `cargo run -p finger-test --bin decode -- logs/hint-v2-capture.png` against a new multi-row capture.
3. **Unit tests**: `cargo test -p finger-core`.
4. **Python sanity**: inspect `logs/hint-v2-raw.txt` via the existing Python script to confirm nibble pairs decode to expected byte values.
