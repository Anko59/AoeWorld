//! Screenshots as pixels: decoding, the scene-only difference and the montage.
//! The difference counts the scene (the canvas) only, with the HUD rectangles
//! that overlap it masked out, because the HUD changes between runs.
use serde::{Deserialize, Serialize};

/// No input image is wider or taller than this (the viewport is far smaller).
pub(crate) const MAX_SIDE: u32 = 4096;
/// An overall montage stays within this many pixels on each side.
pub(crate) const MONTAGE_LIMIT: u32 = 16_384;

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Image {
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// RGBA, row by row.
    pub(crate) rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) w: u32,
    pub(crate) h: u32,
}

impl Rect {
    fn contains(self, x: u32, y: u32) -> bool {
        x >= self.x && y >= self.y && x - self.x < self.w && y - self.y < self.h
    }
}

impl Image {
    pub(crate) fn filled(width: u32, height: u32, color: [u8; 4]) -> Self {
        Self {
            width,
            height,
            rgba: color.repeat(width as usize * height as usize),
        }
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().map_err(|e| format!("png: {e}"))?;
        let info = reader.info();
        if info.width == 0 || info.height == 0 || info.width > MAX_SIDE || info.height > MAX_SIDE {
            return Err(format!(
                "png: {}x{} is outside 1..={MAX_SIDE}",
                info.width, info.height
            ));
        }
        let mut buffer = vec![0; reader.output_buffer_size()];
        let frame = reader
            .next_frame(&mut buffer)
            .map_err(|e| format!("png: {e}"))?;
        let data = &buffer[..frame.buffer_size()];
        let rgba = match frame.color_type {
            png::ColorType::Rgba => data.to_vec(),
            png::ColorType::Rgb => data
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::GrayscaleAlpha => data
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[0], p[0], p[1]])
                .collect(),
            png::ColorType::Indexed => return Err("png: indexed colour was not expanded".into()),
        };
        Ok(Self {
            width: frame.width,
            height: frame.height,
            rgba,
        })
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut w| w.write_image_data(&self.rgba))
            .map_err(|e| format!("png: {e}"))?;
        Ok(out)
    }

    fn at(&self, x: u32, y: u32) -> &[u8] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        &self.rgba[i..i + 4]
    }

    fn set(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x < self.width && y < self.height {
            let i = (y as usize * self.width as usize + x as usize) * 4;
            self.rgba[i..i + 4].copy_from_slice(&color);
        }
    }

    /// The pixels inside `rect`, clamped to the image.
    pub(crate) fn crop(&self, rect: Rect) -> Self {
        let x1 = (rect.x + rect.w).min(self.width);
        let y1 = (rect.y + rect.h).min(self.height);
        let (x0, y0) = (rect.x.min(x1), rect.y.min(y1));
        let mut out = Self::filled(x1 - x0, y1 - y0, [0; 4]);
        for y in y0..y1 {
            let row = (y as usize * self.width as usize + x0 as usize) * 4;
            let to = ((y - y0) as usize * out.width as usize) * 4;
            let len = (x1 - x0) as usize * 4;
            out.rgba[to..to + len].copy_from_slice(&self.rgba[row..row + len]);
        }
        out
    }

    fn paste(&mut self, other: &Self, x: u32, y: u32) {
        for row in 0..other.height {
            if y + row >= self.height || x >= self.width {
                break;
            }
            let len = other.width.min(self.width - x) as usize * 4;
            let from = row as usize * other.width as usize * 4;
            let to = ((y + row) as usize * self.width as usize + x as usize) * 4;
            self.rgba[to..to + len].copy_from_slice(&other.rgba[from..from + len]);
        }
    }

    /// Every `k`-th pixel averaged in `k`×`k` boxes (the contact sheet only).
    fn shrink(&self, k: u32) -> Self {
        if k <= 1 {
            return self.clone();
        }
        let (w, h) = (self.width.div_ceil(k), self.height.div_ceil(k));
        let mut out = Self::filled(w, h, [0; 4]);
        for oy in 0..h {
            for ox in 0..w {
                let mut sum = [0u32; 4];
                let mut n = 0;
                for y in oy * k..((oy + 1) * k).min(self.height) {
                    for x in ox * k..((ox + 1) * k).min(self.width) {
                        for (s, c) in sum.iter_mut().zip(self.at(x, y)) {
                            *s += u32::from(*c);
                        }
                        n += 1;
                    }
                }
                out.set(ox, oy, sum.map(|s| (s / n) as u8));
            }
        }
        out
    }
}

/// What a before/after pair differs by, in the scene only.
#[derive(Debug, PartialEq)]
pub(crate) struct Diff {
    /// Pixels compared: the scene minus the masks.
    pub(crate) compared: u64,
    /// Compared pixels that differ in any channel.
    pub(crate) changed: u64,
    /// Pixels under a mask that differ: reported, never counted.
    pub(crate) masked_changed: u64,
    /// Full-size picture: changed pixels red over a dimmed "after", masked
    /// differences blue.
    pub(crate) image: Image,
}

impl Diff {
    pub(crate) fn percent(&self) -> f64 {
        if self.compared == 0 {
            0.0
        } else {
            self.changed as f64 * 100.0 / self.compared as f64
        }
    }
}

pub(crate) fn diff(before: &Image, after: &Image, scene: Rect, masks: &[Rect]) -> Result<Diff> {
    if (before.width, before.height) != (after.width, after.height) {
        return Err(format!(
            "screenshots differ in size: {}x{} before, {}x{} after",
            before.width, before.height, after.width, after.height
        ));
    }
    let mut image = Image::filled(after.width, after.height, [0, 0, 0, 255]);
    let (mut compared, mut changed, mut masked_changed) = (0, 0, 0);
    for y in 0..after.height {
        for x in 0..after.width {
            let pixel = after.at(x, y);
            let dim = [pixel[0] / 3, pixel[1] / 3, pixel[2] / 3, 255];
            if !scene.contains(x, y) {
                image.set(x, y, dim);
                continue;
            }
            let differs = before.at(x, y) != pixel;
            if masks.iter().any(|m| m.contains(x, y)) {
                masked_changed += u64::from(differs);
                image.set(x, y, if differs { [60, 120, 255, 255] } else { dim });
            } else {
                compared += 1;
                changed += u64::from(differs);
                image.set(x, y, if differs { [255, 40, 40, 255] } else { dim });
            }
        }
    }
    Ok(Diff {
        compared,
        changed,
        masked_changed,
        image,
    })
}

/// 5×7 glyphs, one byte a column, bit 0 on top, for the montage labels.
fn glyph(c: char) -> [u8; 5] {
    match c.to_ascii_uppercase() {
        'A' => [0x7e, 0x11, 0x11, 0x11, 0x7e],
        'B' => [0x7f, 0x49, 0x49, 0x49, 0x36],
        'C' => [0x3e, 0x41, 0x41, 0x41, 0x22],
        'D' => [0x7f, 0x41, 0x41, 0x22, 0x1c],
        'E' => [0x7f, 0x49, 0x49, 0x49, 0x41],
        'F' => [0x7f, 0x09, 0x09, 0x09, 0x01],
        'G' => [0x3e, 0x41, 0x49, 0x49, 0x7a],
        'H' => [0x7f, 0x08, 0x08, 0x08, 0x7f],
        'I' => [0x00, 0x41, 0x7f, 0x41, 0x00],
        'J' => [0x20, 0x40, 0x41, 0x3f, 0x01],
        'K' => [0x7f, 0x08, 0x14, 0x22, 0x41],
        'L' => [0x7f, 0x40, 0x40, 0x40, 0x40],
        'M' => [0x7f, 0x02, 0x0c, 0x02, 0x7f],
        'N' => [0x7f, 0x04, 0x08, 0x10, 0x7f],
        'O' => [0x3e, 0x41, 0x41, 0x41, 0x3e],
        'P' => [0x7f, 0x09, 0x09, 0x09, 0x06],
        'Q' => [0x3e, 0x41, 0x51, 0x21, 0x5e],
        'R' => [0x7f, 0x09, 0x19, 0x29, 0x46],
        'S' => [0x46, 0x49, 0x49, 0x49, 0x31],
        'T' => [0x01, 0x01, 0x7f, 0x01, 0x01],
        'U' => [0x3f, 0x40, 0x40, 0x40, 0x3f],
        'V' => [0x1f, 0x20, 0x40, 0x20, 0x1f],
        'W' => [0x3f, 0x40, 0x38, 0x40, 0x3f],
        'X' => [0x63, 0x14, 0x08, 0x14, 0x63],
        'Y' => [0x07, 0x08, 0x70, 0x08, 0x07],
        'Z' => [0x61, 0x51, 0x49, 0x45, 0x43],
        '0' => [0x3e, 0x51, 0x49, 0x45, 0x3e],
        '1' => [0x00, 0x42, 0x7f, 0x40, 0x00],
        '2' => [0x42, 0x61, 0x51, 0x49, 0x46],
        '3' => [0x21, 0x41, 0x45, 0x4b, 0x31],
        '4' => [0x18, 0x14, 0x12, 0x7f, 0x10],
        '5' => [0x27, 0x45, 0x45, 0x45, 0x39],
        '6' => [0x3c, 0x4a, 0x49, 0x49, 0x30],
        '7' => [0x01, 0x71, 0x09, 0x05, 0x03],
        '8' => [0x36, 0x49, 0x49, 0x49, 0x36],
        '9' => [0x06, 0x49, 0x49, 0x29, 0x1e],
        '-' => [0x08, 0x08, 0x08, 0x08, 0x08],
        '.' => [0x00, 0x60, 0x60, 0x00, 0x00],
        ',' => [0x00, 0x80, 0x60, 0x00, 0x00],
        ':' => [0x00, 0x36, 0x36, 0x00, 0x00],
        '%' => [0x23, 0x13, 0x08, 0x64, 0x62],
        '(' => [0x00, 0x1c, 0x22, 0x41, 0x00],
        ')' => [0x00, 0x41, 0x22, 0x1c, 0x00],
        '|' => [0x00, 0x00, 0x7f, 0x00, 0x00],
        '/' => [0x20, 0x10, 0x08, 0x04, 0x02],
        '=' => [0x14, 0x14, 0x14, 0x14, 0x14],
        '+' => [0x08, 0x08, 0x3e, 0x08, 0x08],
        _ => [0; 5],
    }
}

/// Height in pixels of one label line at the montage's text scale.
pub(crate) const LABEL_HEIGHT: u32 = 28;
const TEXT_SCALE: u32 = 2;

fn text(image: &mut Image, x: u32, y: u32, line: &str, color: [u8; 4]) {
    let mut cursor = x;
    for c in line.chars() {
        for (column, bits) in glyph(c).iter().enumerate() {
            for row in 0..7u32 {
                if bits >> row & 1 == 1 {
                    for dy in 0..TEXT_SCALE {
                        for dx in 0..TEXT_SCALE {
                            image.set(
                                cursor + column as u32 * TEXT_SCALE + dx,
                                y + row * TEXT_SCALE + dy,
                                color,
                            );
                        }
                    }
                }
            }
        }
        cursor += 6 * TEXT_SCALE;
    }
}

/// One view: a label line, then `panels` side by side at 1:1 scale, each with
/// its own caption in the label line.
pub(crate) fn row(title: &str, panels: &[(&str, &Image)]) -> Image {
    const GAP: u32 = 6;
    let height = panels.iter().map(|(_, i)| i.height).max().unwrap_or(0);
    let width: u32 = panels
        .iter()
        .map(|(_, i)| i.width + GAP)
        .sum::<u32>()
        .saturating_sub(GAP);
    let mut sheet = Image::filled(width.max(1), LABEL_HEIGHT + height, [24, 28, 34, 255]);
    let mut left = 0;
    for (caption, image) in panels {
        let label = if left == 0 {
            format!("{title}  {caption}")
        } else {
            (*caption).to_string()
        };
        text(&mut sheet, left + 6, 7, &label, [230, 237, 243, 255]);
        sheet.paste(image, left, LABEL_HEIGHT);
        left += image.width + GAP;
    }
    sheet
}

/// Rows stacked into the contact sheet, each shrunk by the same whole factor
/// so that no side exceeds [`MONTAGE_LIMIT`]; returns the sheet and the factor.
pub(crate) fn stack(rows: &[Image]) -> (Image, u32) {
    const SEAM: u32 = 4;
    let width = rows.iter().map(|r| r.width).max().unwrap_or(1);
    let height: u32 = rows.iter().map(|r| r.height + SEAM).sum::<u32>().max(1);
    let factor = width.max(height).div_ceil(MONTAGE_LIMIT).max(1);
    let shrunk: Vec<Image> = rows.iter().map(|r| r.shrink(factor)).collect();
    let (w, h) = (
        shrunk.iter().map(|r| r.width).max().unwrap_or(1),
        shrunk.iter().map(|r| r.height + 1).sum::<u32>().max(1),
    );
    let mut sheet = Image::filled(w, h, [10, 12, 15, 255]);
    let mut top = 0;
    for r in &shrunk {
        sheet.paste(r, 0, top);
        top += r.height + 1;
    }
    (sheet, factor)
}
