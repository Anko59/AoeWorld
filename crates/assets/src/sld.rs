//! Bounded SLD (`SLDX` version 4) decoder, written from the public format
//! description: a 16-byte header, per-frame headers, and up to five layers per
//! frame. The main layer is BC1 (DXT1), the shadow and player-colour layers are
//! BC4, and the damage-mask layers are skipped by length. A layer with the
//! "reuse previous frame" flag fills its skipped blocks from the same layer of
//! the preceding frame. Every count, size and offset is checked before use, so
//! malformed input yields an [`Error`], never a panic.
use crate::{Error, invalid, slice, u16_at, u32_at};

const FORMAT: &str = "SLD";
const HEADER: usize = 16;
const FRAME_HEADER: usize = 12;
const MAX_FILE: usize = 256 * 1024 * 1024;
const MAX_FRAMES: usize = 4_096;
const MAX_SIDE: usize = 2_048;
/// Decoded layer pixels summed over one file (four bytes each).
const MAX_TOTAL_PIXELS: usize = 48 * 1024 * 1024;
const REUSE_FLAG: u8 = 0x80;

/// One decoded layer positioned inside its frame canvas. Main-layer pixels are
/// RGBA; BC4 layers carry their 8-bit intensity in the red channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layer {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<[u8; 4]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub width: u16,
    pub height: u16,
    pub anchor_x: u16,
    pub anchor_y: u16,
    pub index: u16,
    pub main: Option<Layer>,
    pub shadow: Option<Layer>,
    pub player: Option<Layer>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Kind {
    Main,
    Shadow,
    Skipped,
    Player,
}

/// Bit order of the frame's layer flags, with the layer's kind.
const LAYERS: [Kind; 5] = [
    Kind::Main,
    Kind::Shadow,
    Kind::Skipped, // unknown layer
    Kind::Skipped, // damage mask
    Kind::Player,
];

fn rgb565(value: u16) -> [u8; 4] {
    let red = ((value >> 11) & 31) as u8;
    let green = ((value >> 5) & 63) as u8;
    let blue = (value & 31) as u8;
    [
        (red << 3) | (red >> 2),
        (green << 2) | (green >> 4),
        (blue << 3) | (blue >> 2),
        255,
    ]
}

/// Decodes one 8-byte BC1 block into 16 RGBA pixels in row-major order.
pub fn bc1_block(block: [u8; 8]) -> [[u8; 4]; 16] {
    let a = u16::from_le_bytes([block[0], block[1]]);
    let b = u16::from_le_bytes([block[2], block[3]]);
    let (c0, c1) = (rgb565(a), rgb565(b));
    let mut table = [c0, c1, [0, 0, 0, 255], [0; 4]];
    for k in 0..3 {
        let (p, q) = (u16::from(c0[k]), u16::from(c1[k]));
        if a > b {
            table[2][k] = ((2 * p + q + 1) / 3) as u8;
            table[3][k] = ((p + 2 * q + 1) / 3) as u8;
        } else {
            table[2][k] = ((p + q) / 2) as u8;
        }
    }
    if a > b {
        table[3][3] = 255;
    }
    let mut bits = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    let mut out = [[0; 4]; 16];
    for pixel in &mut out {
        *pixel = table[(bits & 3) as usize];
        bits >>= 2;
    }
    out
}

/// Decodes one 8-byte BC4 block into 16 intensities in row-major order.
pub fn bc4_block(block: [u8; 8]) -> [u8; 16] {
    let (a, b) = (u16::from(block[0]), u16::from(block[1]));
    let mut table = [a, b, 0, 0, 0, 0, 0, 0];
    if a > b {
        for i in 1..=6u16 {
            table[i as usize + 1] = ((7 - i) * a + i * b) / 7;
        }
    } else {
        for i in 1..=4u16 {
            table[i as usize + 1] = ((5 - i) * a + i * b) / 5;
        }
        table[7] = 255;
    }
    let mut bits = 0u64;
    for (i, byte) in block[2..].iter().enumerate() {
        bits |= u64::from(*byte) << (8 * i);
    }
    let mut out = [0; 16];
    for value in &mut out {
        *value = table[(bits & 7) as usize] as u8;
        bits >>= 3;
    }
    out
}

struct Rect {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

/// Reads one layer starting at `start` (its length word). Returns the layer, if
/// decoded, and the offset of the next layer.
#[allow(clippy::too_many_arguments)]
fn layer(
    bytes: &[u8],
    start: usize,
    kind: Kind,
    canvas: (usize, usize),
    main: &Rect,
    previous: Option<&Layer>,
    pixel_budget: &mut usize,
) -> Result<(Option<Layer>, usize), Error> {
    let length = u32_at(bytes, start, FORMAT)? as usize;
    let end = start
        .checked_add(length)
        .filter(|end| *end <= bytes.len() && length >= 6)
        .ok_or_else(|| invalid(FORMAT, start, "layer length outside file"))?;
    if kind == Kind::Skipped {
        return Ok((None, end));
    }
    let mut pos = start + 4;
    let (rect, flags) = if matches!(kind, Kind::Main | Kind::Shadow) {
        let x = usize::from(u16_at(bytes, pos, FORMAT)?);
        let y = usize::from(u16_at(bytes, pos + 2, FORMAT)?);
        let right = usize::from(u16_at(bytes, pos + 4, FORMAT)?);
        let bottom = usize::from(u16_at(bytes, pos + 6, FORMAT)?);
        let flags = slice(bytes, pos + 8, 1, FORMAT)?[0];
        pos += 10;
        if right < x || bottom < y {
            return Err(invalid(FORMAT, start, "layer rectangle is inverted"));
        }
        (
            Rect {
                x,
                y,
                width: right - x,
                height: bottom - y,
            },
            flags,
        )
    } else {
        let flags = slice(bytes, pos, 1, FORMAT)?[0];
        pos += 2;
        (
            Rect {
                x: main.x,
                y: main.y,
                width: main.width,
                height: main.height,
            },
            flags,
        )
    };
    if rect.width % 4 != 0
        || rect.height % 4 != 0
        || rect.x + rect.width > canvas.0.next_multiple_of(4)
        || rect.y + rect.height > canvas.1.next_multiple_of(4)
    {
        return Err(invalid(FORMAT, start, "layer outside block-padded canvas"));
    }
    let pixel_count = rect.width * rect.height;
    *pixel_budget = pixel_budget
        .checked_sub(pixel_count)
        .ok_or_else(|| invalid(FORMAT, start, "decoded pixel budget exceeded"))?;
    let commands = usize::from(u16_at(bytes, pos, FORMAT)?);
    pos += 2;
    let command_bytes = slice(bytes, pos, commands * 2, FORMAT)?;
    let mut data = pos + commands * 2;
    if data > end {
        return Err(invalid(FORMAT, start, "commands cross layer end"));
    }
    let blocks_wide = rect.width / 4;
    let blocks = blocks_wide * (rect.height / 4);
    let reuse = if flags & REUSE_FLAG != 0 {
        previous
    } else {
        None
    };
    let mut pixels = vec![[0u8; 4]; pixel_count];
    let mut ordinal = 0usize;
    for pair in command_bytes.as_chunks::<2>().0 {
        let (skip, draw) = (usize::from(pair[0]), usize::from(pair[1]));
        if ordinal + skip + draw > blocks {
            return Err(invalid(FORMAT, start, "commands exceed layer blocks"));
        }
        for _ in 0..skip {
            if let Some(old) = reuse {
                copy_block(&mut pixels, &rect, blocks_wide, ordinal, old);
            }
            ordinal += 1;
        }
        for _ in 0..draw {
            let raw = slice(bytes, data, 8, FORMAT)?;
            if data + 8 > end {
                return Err(invalid(FORMAT, data, "block data crosses layer end"));
            }
            let mut block = [0u8; 8];
            block.copy_from_slice(raw);
            data += 8;
            let decoded = if kind == Kind::Main {
                bc1_block(block)
            } else {
                bc4_block(block).map(|value| [value, 0, 0, 255])
            };
            let (bx, by) = (ordinal % blocks_wide * 4, ordinal / blocks_wide * 4);
            for (i, pixel) in decoded.iter().enumerate() {
                pixels[(by + i / 4) * rect.width + bx + i % 4] = *pixel;
            }
            ordinal += 1;
        }
    }
    // A sparse command list may stop early. Unmentioned tail blocks remain
    // transparent; only explicit skip commands can reuse previous pixels.
    if data != end {
        return Err(invalid(FORMAT, start, "unused compressed layer payload"));
    }
    let to_u16 = |value: usize| value as u16; // every value is at most 65535
    Ok((
        Some(Layer {
            x: to_u16(rect.x),
            y: to_u16(rect.y),
            width: to_u16(rect.width),
            height: to_u16(rect.height),
            pixels,
        }),
        end,
    ))
}

/// Copies a 4x4 block from the previous frame's layer, in canvas coordinates.
fn copy_block(
    pixels: &mut [[u8; 4]],
    rect: &Rect,
    blocks_wide: usize,
    ordinal: usize,
    old: &Layer,
) {
    let (bx, by) = (ordinal % blocks_wide * 4, ordinal / blocks_wide * 4);
    for dy in 0..4 {
        for dx in 0..4 {
            let ox = (rect.x + bx + dx).checked_sub(usize::from(old.x));
            let oy = (rect.y + by + dy).checked_sub(usize::from(old.y));
            if let (Some(ox), Some(oy)) = (ox, oy)
                && ox < usize::from(old.width)
                && oy < usize::from(old.height)
            {
                pixels[(by + dy) * rect.width + bx + dx] =
                    old.pixels[oy * usize::from(old.width) + ox];
            }
        }
    }
}

fn header(bytes: &[u8]) -> Result<(usize, usize), Error> {
    if bytes.len() > MAX_FILE {
        return Err(invalid(FORMAT, 0, "file exceeds size limit"));
    }
    if slice(bytes, 0, 4, FORMAT)? != b"SLDX" {
        return Err(invalid(FORMAT, 0, "signature is not SLDX"));
    }
    if u16_at(bytes, 4, FORMAT)? != 4 {
        return Err(Error::Unsupported {
            format: FORMAT,
            detail: "only SLDX version 4 is supported".to_owned(),
        });
    }
    let count = usize::from(u16_at(bytes, 6, FORMAT)?);
    if count > MAX_FRAMES {
        return Err(invalid(FORMAT, 6, "too many frames"));
    }
    let header = usize::from(u16_at(bytes, 10, FORMAT)?);
    if ![14, HEADER].contains(&header) {
        return Err(invalid(FORMAT, 10, "unexpected header size"));
    }
    Ok((count, header))
}

fn decode_frame(
    bytes: &[u8],
    mut pos: usize,
    header: usize,
    previous: Option<&Frame>,
    budget: &mut usize,
) -> Result<(Frame, usize), Error> {
    let width = usize::from(u16_at(bytes, pos, FORMAT)?);
    let height = usize::from(u16_at(bytes, pos + 2, FORMAT)?);
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(invalid(FORMAT, pos, "frame dimensions exceed limit"));
    }
    let anchor_x = u16_at(bytes, pos + 4, FORMAT)?;
    let anchor_y = u16_at(bytes, pos + 6, FORMAT)?;
    let flags = slice(bytes, pos + 8, 1, FORMAT)?[0];
    let index = u16_at(bytes, pos + 10, FORMAT)?;
    pos += FRAME_HEADER;
    let mut main = Rect {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };
    let mut decoded: [Option<Layer>; 5] = Default::default();
    for (bit, kind) in LAYERS.into_iter().enumerate() {
        if flags & (1 << bit) == 0 {
            continue;
        }
        if bit > 0 && decoded[0].is_none() && kind == Kind::Player {
            return Err(invalid(FORMAT, pos, "player layer without main layer"));
        }
        let previous = (index > 0)
            .then_some(previous)
            .flatten()
            .and_then(|frame| match kind {
                Kind::Main => frame.main.as_ref(),
                Kind::Shadow => frame.shadow.as_ref(),
                Kind::Player => frame.player.as_ref(),
                Kind::Skipped => None,
            });
        let (found, next) = layer(bytes, pos, kind, (width, height), &main, previous, budget)?;
        if let Some(found) = &found
            && kind == Kind::Main
        {
            main = Rect {
                x: usize::from(found.x),
                y: usize::from(found.y),
                width: usize::from(found.width),
                height: usize::from(found.height),
            };
        }
        decoded[bit] = found;
        pos = next;
        pos += (4 - (pos - header) % 4) % 4;
    }
    let [main, shadow, _, _, player] = decoded;
    let frame = Frame {
        width: width as u16,
        height: height as u16,
        anchor_x,
        anchor_y,
        index,
        main,
        shadow,
        player,
    };
    Ok((frame, pos))
}

/// Decodes every frame while retaining the original whole-file memory bound.
pub fn parse(bytes: &[u8]) -> Result<Vec<Frame>, Error> {
    let (count, header) = header(bytes)?;
    let mut pos = header;
    let mut budget = MAX_TOTAL_PIXELS;
    let mut frames = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        let (frame, next) = decode_frame(bytes, pos, header, frames.last(), &mut budget)?;
        frames.push(frame);
        pos = next;
    }
    if pos != bytes.len() {
        return Err(invalid(FORMAT, pos, "unparsed trailing bytes"));
    }
    Ok(frames)
}
#[path = "sld/stream.rs"]
mod stream;
pub use stream::Stream;

#[path = "sld/tests.rs"]
#[cfg(test)]
mod tests;
