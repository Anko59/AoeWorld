//! Bounded SLP 2.0N frame decoder with independent player, shadow, and outline pixels.
use crate::{Error, i32_at, invalid, slice, u16_at, u32_at};

const FORMAT: &str = "SLP 2.0N";
const MAX_FILE: usize = 64 * 1024 * 1024;
const MAX_FRAMES: usize = 4_096;
const MAX_PIXELS: usize = 16_777_216;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pixel {
    Transparent,
    Color(u8),
    Player(u8),
    Shadow,
    Outline1,
    Outline2,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub width: u16,
    pub height: u16,
    pub hotspot_x: i32,
    pub hotspot_y: i32,
    pub pixels: Vec<Pixel>,
}

fn byte(bytes: &[u8], cursor: &mut usize) -> Result<u8, Error> {
    let value = *bytes
        .get(*cursor)
        .ok_or_else(|| invalid(FORMAT, *cursor, "truncated command"))?;
    *cursor += 1;
    Ok(value)
}

fn variable_count(cmd: u8, bytes: &[u8], cursor: &mut usize) -> Result<usize, Error> {
    let top = (cmd >> 4) as usize;
    Ok(if top == 0 {
        byte(bytes, cursor)? as usize
    } else {
        top
    })
}

fn paint(
    frame: &mut Frame,
    row: usize,
    x: &mut usize,
    end: usize,
    pixels: impl IntoIterator<Item = Pixel>,
) -> Result<(), Error> {
    for pixel in pixels {
        if *x >= end {
            return Err(invalid(FORMAT, row, "draw crosses row boundary"));
        }
        frame.pixels[row * frame.width as usize + *x] = pixel;
        *x += 1;
    }
    Ok(())
}

fn skip(x: &mut usize, count: usize, end: usize, offset: usize) -> Result<(), Error> {
    *x = x
        .checked_add(count)
        .ok_or_else(|| invalid(FORMAT, offset, "skip overflow"))?;
    if *x > end {
        return Err(invalid(FORMAT, offset, "skip crosses row boundary"));
    }
    Ok(())
}

fn read_pixels(
    bytes: &[u8],
    cursor: &mut usize,
    count: usize,
    player: bool,
) -> Result<Vec<Pixel>, Error> {
    let data = slice(bytes, *cursor, count, FORMAT)?;
    *cursor += count;
    Ok(data
        .iter()
        .copied()
        .map(|value| {
            if player {
                Pixel::Player(value)
            } else {
                Pixel::Color(value)
            }
        })
        .collect())
}

fn decode_row(
    bytes: &[u8],
    frame: &mut Frame,
    row: usize,
    left: usize,
    right: usize,
    mut cursor: usize,
) -> Result<(), Error> {
    let end = frame.width as usize - right;
    let mut x = left;
    let mut commands = 0usize;
    loop {
        commands += 1;
        if commands > frame.width as usize * 2 + 32 {
            return Err(invalid(FORMAT, cursor, "too many row commands"));
        }
        let offset = cursor;
        let command = byte(bytes, &mut cursor)?;
        if command == 0x0F {
            if x != end {
                return Err(invalid(FORMAT, offset, "row ended before declared edge"));
            }
            return Ok(());
        }
        match command & 0x0F {
            _ if command & 0x03 == 0 => {
                let count = (command >> 2) as usize;
                if count == 0 {
                    return Err(invalid(FORMAT, offset, "empty draw"));
                }
                let pixels = read_pixels(bytes, &mut cursor, count, false)?;
                paint(frame, row, &mut x, end, pixels)?;
            }
            _ if command & 0x03 == 1 => {
                let mut count = (command >> 2) as usize;
                if count == 0 {
                    count = byte(bytes, &mut cursor)? as usize;
                }
                if count == 0 {
                    return Err(invalid(FORMAT, offset, "empty skip"));
                }
                skip(&mut x, count, end, offset)?;
            }
            0x02 => {
                let count = ((command as usize & 0xF0) << 4) | byte(bytes, &mut cursor)? as usize;
                if count == 0 {
                    return Err(invalid(FORMAT, offset, "empty greater draw"));
                }
                let pixels = read_pixels(bytes, &mut cursor, count, false)?;
                paint(frame, row, &mut x, end, pixels)?;
            }
            0x03 => {
                let count = ((command as usize & 0xF0) << 4) | byte(bytes, &mut cursor)? as usize;
                if count == 0 {
                    return Err(invalid(FORMAT, offset, "empty greater skip"));
                }
                skip(&mut x, count, end, offset)?;
            }
            0x06 => {
                let count = variable_count(command, bytes, &mut cursor)?;
                let pixels = read_pixels(bytes, &mut cursor, count, true)?;
                paint(frame, row, &mut x, end, pixels)?;
            }
            0x07 | 0x0A => {
                let count = variable_count(command, bytes, &mut cursor)?;
                let value = byte(bytes, &mut cursor)?;
                let pixel = if command & 0x0F == 0x0A {
                    Pixel::Player(value)
                } else {
                    Pixel::Color(value)
                };
                paint(frame, row, &mut x, end, std::iter::repeat_n(pixel, count))?;
            }
            0x0B => {
                let count = variable_count(command, bytes, &mut cursor)?;
                paint(
                    frame,
                    row,
                    &mut x,
                    end,
                    std::iter::repeat_n(Pixel::Shadow, count),
                )?;
            }
            0x0E => {
                let (pixel, count) = match command {
                    0x4E => (Pixel::Outline1, 1),
                    0x5E => (Pixel::Outline1, byte(bytes, &mut cursor)? as usize),
                    0x6E => (Pixel::Outline2, 1),
                    0x7E => (Pixel::Outline2, byte(bytes, &mut cursor)? as usize),
                    _ => {
                        return Err(Error::Unsupported {
                            format: FORMAT,
                            detail: format!("command 0x{command:02X} at byte {offset}"),
                        });
                    }
                };
                paint(frame, row, &mut x, end, std::iter::repeat_n(pixel, count))?;
            }
            _ => {
                return Err(Error::Unsupported {
                    format: FORMAT,
                    detail: format!("command 0x{command:02X} at byte {offset}"),
                });
            }
        }
    }
}

pub fn parse(bytes: &[u8]) -> Result<Vec<Frame>, Error> {
    if bytes.len() > MAX_FILE {
        return Err(invalid(FORMAT, 0, "sprite exceeds 64 MiB"));
    }
    if slice(bytes, 0, 4, FORMAT)? != b"2.0N" {
        return Err(Error::Unsupported {
            format: FORMAT,
            detail: "expected version 2.0N".to_owned(),
        });
    }
    let count = u32_at(bytes, 4, FORMAT)? as usize;
    if count == 0 || count > MAX_FRAMES {
        return Err(invalid(FORMAT, 4, "frame count out of bounds"));
    }
    slice(
        bytes,
        32,
        count
            .checked_mul(32)
            .ok_or_else(|| invalid(FORMAT, 32, "frame table overflow"))?,
        FORMAT,
    )?;
    let mut total_pixels = 0usize;
    let mut frames = Vec::with_capacity(count);
    for index in 0..count {
        let info = 32 + index * 32;
        let command_table = u32_at(bytes, info, FORMAT)? as usize;
        let outline_table = u32_at(bytes, info + 4, FORMAT)? as usize;
        let width = i32_at(bytes, info + 16, FORMAT)?;
        let height = i32_at(bytes, info + 20, FORMAT)?;
        if !(1..=4_096).contains(&width) || !(1..=4_096).contains(&height) {
            return Err(invalid(FORMAT, info + 16, "frame dimensions out of bounds"));
        }
        let pixels = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| invalid(FORMAT, info, "pixel count overflow"))?;
        total_pixels = total_pixels
            .checked_add(pixels)
            .ok_or_else(|| invalid(FORMAT, info, "total pixels overflow"))?;
        if total_pixels > MAX_PIXELS {
            return Err(invalid(FORMAT, info, "sprite exceeds 16M pixels"));
        }
        slice(bytes, outline_table, height as usize * 4, FORMAT)?;
        slice(bytes, command_table, height as usize * 4, FORMAT)?;
        let mut frame = Frame {
            width: width as u16,
            height: height as u16,
            hotspot_x: i32_at(bytes, info + 24, FORMAT)?,
            hotspot_y: i32_at(bytes, info + 28, FORMAT)?,
            pixels: vec![Pixel::Transparent; pixels],
        };
        for row in 0..height as usize {
            let edge = outline_table + row * 4;
            let left = u16_at(bytes, edge, FORMAT)?;
            let right = u16_at(bytes, edge + 2, FORMAT)?;
            if left == 0x8000 || right == 0x8000 {
                continue;
            }
            if usize::from(left) + usize::from(right) > width as usize {
                return Err(invalid(FORMAT, edge, "edge exceeds width"));
            }
            let command_offset = u32_at(bytes, command_table + row * 4, FORMAT)? as usize;
            decode_row(
                bytes,
                &mut frame,
                row,
                left as usize,
                right as usize,
                command_offset,
            )?;
        }
        frames.push(frame);
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(command: &[u8], width: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; 72];
        bytes[..4].copy_from_slice(b"2.0N");
        bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&68u32.to_le_bytes());
        bytes[36..40].copy_from_slice(&64u32.to_le_bytes());
        bytes[48..52].copy_from_slice(&width.to_le_bytes());
        bytes[52..56].copy_from_slice(&1u32.to_le_bytes());
        bytes[56..60].copy_from_slice(&1u32.to_le_bytes());
        bytes[68..72].copy_from_slice(&72u32.to_le_bytes());
        bytes.extend_from_slice(command);
        bytes
    }

    #[test]
    fn fixture_color_and_masks() {
        let color = parse(&fixture(&[0x08, 5, 6, 0x0F], 2)).unwrap();
        assert_eq!(color[0].pixels, vec![Pixel::Color(5), Pixel::Color(6)]);
        assert_eq!(color[0].hotspot_x, 1);
        let mask = parse(&fixture(&[0x16, 7, 0x1B, 0x4E, 0x0F], 3)).unwrap();
        assert_eq!(
            mask[0].pixels,
            vec![Pixel::Player(7), Pixel::Shadow, Pixel::Outline1]
        );
    }

    #[test]
    fn invalid_data_fails() {
        assert!(parse(&fixture(&[0x08, 5, 6], 2)).is_err());
        assert!(parse(&fixture(&[0x8E, 0x0F], 1)).is_err());
        assert!(parse(&fixture(&[0x08, 5, 6, 0x0F], 1)).is_err());
    }
}
