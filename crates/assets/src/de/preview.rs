//! PNG previews of the imported sprites for visual review (local only).
use super::{compose, farm::Sheet};
use crate::{Error, pack::atlas::Planes, sld::Frame};
use std::{fs, io::BufWriter, path::Path};

const GROUND: [u8; 3] = [58, 92, 50];
const COLUMNS: usize = 4;
const MAX_CELLS: usize = 12;
const PAD: usize = 6;

/// Player colour as the client draws it: the blue ramp scaled by the ramp value.
fn player_rgb(ramp: u8) -> [u8; 3] {
    let shade = 0.65 + f32::from(ramp.min(7)) / 7.0 * 0.35;
    [
        (65.0 * shade) as u8,
        (145.0 * shade) as u8,
        (245.0 * shade) as u8,
    ]
}

/// Composites planes over the ground colour in the client's precedence: player
/// colour, then colour, then shadow. Returns RGB and marks the anchor.
fn render(planes: &Planes) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(usize::from(planes.width) * usize::from(planes.height) * 3);
    for at in 0..usize::from(planes.width) * usize::from(planes.height) {
        let (p, c, s) = (
            &planes.player[at * 4..at * 4 + 4],
            &planes.color[at * 4..at * 4 + 4],
            &planes.shadow[at * 4..at * 4 + 4],
        );
        if p[3] > 0 {
            rgb.extend_from_slice(&player_rgb(p[0]));
        } else if c[3] > 0 {
            rgb.extend_from_slice(&c[..3]);
        } else {
            let a = u32::from(s[3]);
            rgb.extend(GROUND.map(|v| (u32::from(v) * (255 - a) / 255) as u8));
        }
    }
    rgb
}

fn sheet(frames: &[Planes]) -> (u32, u32, Vec<u8>) {
    let shown = &frames[..frames.len().min(MAX_CELLS)];
    let cell_w = shown
        .iter()
        .map(|f| usize::from(f.width))
        .max()
        .unwrap_or(1)
        + PAD;
    let cell_h = shown
        .iter()
        .map(|f| usize::from(f.height))
        .max()
        .unwrap_or(1)
        + PAD;
    let columns = shown.len().clamp(1, COLUMNS);
    let rows = shown.len().div_ceil(columns).max(1);
    let (width, height) = (columns * cell_w, rows * cell_h);
    let mut image = GROUND.repeat(width * height);
    for (index, frame) in shown.iter().enumerate() {
        let (ox, oy) = (
            index % columns * cell_w + PAD / 2,
            index / columns * cell_h + PAD / 2,
        );
        let rgb = render(frame);
        for row in 0..usize::from(frame.height) {
            let to = ((oy + row) * width + ox) * 3;
            let from = row * usize::from(frame.width) * 3;
            image[to..to + usize::from(frame.width) * 3]
                .copy_from_slice(&rgb[from..from + usize::from(frame.width) * 3]);
        }
        // Magenta cross at the anchor, if it falls inside the cell.
        let (ax, ay) = (frame.hotspot_x, frame.hotspot_y);
        for d in -3i32..=3 {
            for (px, py) in [(ax + d, ay), (ax, ay + d)] {
                let (x, y) = (px + ox as i32, py + oy as i32);
                if x >= 0 && y >= 0 && (x as usize) < width && (y as usize) < height {
                    let to = (y as usize * width + x as usize) * 3;
                    image[to..to + 3].copy_from_slice(&[255, 0, 255]);
                }
            }
        }
    }
    (width as u32, height as u32, image)
}

pub(super) fn write(dir: &Path, sprites: &[(&str, &[Frame])], farm: &[Sheet]) -> Result<(), Error> {
    fs::create_dir_all(dir)?;
    for (stem, frames) in sprites {
        let shown: Vec<Planes> = frames.iter().take(MAX_CELLS).map(compose::planes).collect();
        let (width, height, image) = sheet(&shown);
        save(&dir.join(format!("{stem}.png")), width, height, &image)?;
    }
    for sheet in farm {
        let (width, height, image) = sheet.preview();
        save(
            &dir.join(format!("{}.png", sheet.name)),
            width,
            height,
            &image,
        )?;
    }
    Ok(())
}

fn save(path: &Path, width: u32, height: u32, rgb: &[u8]) -> Result<(), Error> {
    let mut encoder = png::Encoder::new(BufWriter::new(fs::File::create(path)?), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgb)?;
    Ok(())
}
