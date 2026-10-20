//! PNG previews of the imported sprites for visual review (local only).
use super::{compose, farm::Sheet};
use crate::{
    Error,
    pack::{atlas::Planes, blend_player},
    sld::Frame,
};
use std::{fs, io::BufWriter, path::Path};

const GROUND: [u8; 3] = [58, 92, 50];
const COLUMNS: usize = 4;
const MAX_CELLS: usize = 12;
const PAD: usize = 6;

/// Decode the complete DE Town Center at native x1 scale, beside the trial
/// composite. Both use a 96x48 source tile and the same (256,256) ground origin.
/// These licensed proof pixels must remain below the ignored cache directory.
pub fn reference(install: &Path, output: &Path) -> Result<(), Error> {
    if !output.starts_with(".cache")
        || output
            .components()
            .any(|c| c == std::path::Component::ParentDir)
    {
        return Err(crate::invalid(
            "DE reference",
            0,
            "output must stay below .cache",
        ));
    }
    let source = install
        .join(super::GRAPHICS)
        .join("b_dark_town_center_age1_x1.sld");
    let frames = crate::sld::parse(&super::read_bounded(&source)?)?;
    let frame = frames
        .first()
        .ok_or_else(|| crate::invalid("DE reference", 0, "empty sprite"))?;
    let planes = compose::planes(frame);
    let (w, h) = (512usize, 384usize);
    let mut rgb = vec![128; w * h * 3];
    for y in 0..i32::from(planes.height) {
        for x in 0..i32::from(planes.width) {
            let (dx, dy) = (256 - planes.hotspot_x + x, 256 - planes.hotspot_y + y);
            if dx < 0 || dy < 0 || dx >= w as i32 || dy >= h as i32 {
                continue;
            }
            let at = (y as usize * usize::from(planes.width) + x as usize) * 4;
            let to = (dy as usize * w + dx as usize) * 3;
            let c = &planes.color[at..at + 4];
            let p = &planes.player[at..at + 4];
            let color = blend_player([c[0], c[1], c[2], c[3]], p[0], p[3], [65, 145, 245]);
            let shadow = u32::from(planes.shadow[at + 3]);
            for channel in 0..3 {
                let ground = 128 * (255 - shadow) / 255;
                rgb[to + channel] = ((u32::from(color[channel]) * u32::from(color[3])
                    + ground * u32::from(255 - color[3])
                    + 127)
                    / 255) as u8;
            }
        }
    }
    fs::create_dir_all(output)?;
    save(&output.join("reference-de.png"), w as u32, h as u32, &rgb)?;
    let mut decoder = png::Decoder::new(fs::File::open(output.join("tc-composite.png"))?);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    if reader.info().width != w as u32 || reader.info().height != h as u32 {
        return Err(crate::invalid(
            "DE reference",
            0,
            "expected native 512x384 composite",
        ));
    }
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;
    if info.width != w as u32
        || info.height != h as u32
        || !matches!(info.color_type, png::ColorType::Rgb | png::ColorType::Rgba)
    {
        return Err(crate::invalid(
            "DE reference",
            0,
            "expected native 512x384 composite",
        ));
    }
    let channels = if info.color_type == png::ColorType::Rgb {
        3
    } else {
        4
    };
    let mut pair = vec![128; w * 2 * h * 3];
    for y in 0..h {
        pair[y * w * 6..y * w * 6 + w * 3].copy_from_slice(&rgb[y * w * 3..(y + 1) * w * 3]);
        for x in 0..w {
            let at = (y * w + x) * channels;
            let to = (y * w * 2 + w + x) * 3;
            pair[to..to + 3].copy_from_slice(&pixels[at..at + 3]);
        }
    }
    save(
        &output.join("orientation-check.png"),
        (w * 2) as u32,
        h as u32,
        &pair,
    )?;
    println!(
        "DE reference: {}x{}, anchor ({},{}); left DE / right trial; 96x48 source tiles",
        planes.width, planes.height, planes.hotspot_x, planes.hotspot_y
    );
    Ok(())
}

/// Blends player colour over the body, with shadow on exposed ground, using
/// the client's player-plane composition. Returns RGB.
fn render(planes: &Planes) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(usize::from(planes.width) * usize::from(planes.height) * 3);
    for at in 0..usize::from(planes.width) * usize::from(planes.height) {
        let (p, c, s) = (
            &planes.player[at * 4..at * 4 + 4],
            &planes.color[at * 4..at * 4 + 4],
            &planes.shadow[at * 4..at * 4 + 4],
        );
        let base = if c[3] > 0 {
            [c[0], c[1], c[2]]
        } else {
            let a = u32::from(s[3]);
            GROUND.map(|v| (u32::from(v) * (255 - a) / 255) as u8)
        };
        let blended = blend_player([base[0], base[1], base[2], 255], p[0], p[3], [65, 145, 245]);
        rgb.extend_from_slice(&blended[..3]);
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

#[path = "preview/tests.rs"]
#[cfg(test)]
mod tests;
