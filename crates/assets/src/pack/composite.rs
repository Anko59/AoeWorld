//! Local native-scale building proof, using exactly the pack's recipe and planes.
use super::{Manifest, blend_player};
use crate::{Error, dat::BuildingComposition, invalid};
use std::{fs, io::Cursor, path::Path};

fn plane(pack: &Path, name: &str) -> Result<Vec<u8>, Error> {
    let bytes = fs::read(pack.join(name))?;
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut pixels)?;
    Ok(pixels)
}

/// Fixed native pixel canvas and origin make two recipes directly comparable.
/// The caller must verify the manifest before using its page paths.
pub fn render(
    pack: &Path,
    manifest: &Manifest,
    recipe: &BuildingComposition,
    output: &Path,
) -> Result<(), Error> {
    let (width, height) = (512usize, 384usize);
    let mut pixels = vec![128; width * height * 4];
    for p in pixels.as_chunks_mut::<4>().0 {
        p[3] = 255;
    }
    for part in &recipe.parts {
        if part.offset.iter().any(|v| v.unsigned_abs() > 32768)
            || part
                .misplacement
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 128.0)
        {
            return Err(invalid("composite", 0, "invalid part displacement"));
        }
        if !part.available {
            continue;
        }
        let frame = manifest
            .frames
            .iter()
            .find(|f| f.source == part.source && f.frame == 0)
            .ok_or_else(|| invalid("composite", 0, format!("missing {}", part.source)))?;
        let page = &manifest.pages[usize::from(frame.page)];
        let color = plane(pack, &page.color)?;
        let player = plane(pack, &page.player)?;
        let shadow = plane(pack, &page.shadow)?;
        let [ne, se] = part.misplacement;
        let offset = [(ne + se) * 48.0, (se - ne) * 24.0];
        let left = 256 - frame.anchor_x + part.offset[0] + offset[0].round() as i32;
        let top = 256 - frame.anchor_y + part.offset[1] + offset[1].round() as i32;
        for y in 0..i32::from(frame.height) {
            for x in 0..i32::from(frame.width) {
                let (dx, dy) = (left + x, top + y);
                if dx < 0 || dy < 0 || dx >= width as i32 || dy >= height as i32 {
                    continue;
                }
                let at = ((usize::from(frame.y) + y as usize) * usize::from(page.width)
                    + usize::from(frame.x)
                    + x as usize)
                    * 4;
                let mut sample = [color[at], color[at + 1], color[at + 2], color[at + 3]];
                if shadow[at + 3] != 0 {
                    sample = [0, 0, 0, shadow[at + 3]];
                }
                sample = blend_player(sample, player[at], player[at + 3], [65, 145, 245]);
                let to = (dy as usize * width + dx as usize) * 4;
                for channel in 0..3 {
                    pixels[to + channel] = ((u32::from(sample[channel]) * u32::from(sample[3])
                        + u32::from(pixels[to + channel]) * u32::from(255 - sample[3])
                        + 127)
                        / 255) as u8;
                }
            }
        }
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut encoder = png::Encoder::new(fs::File::create(output)?, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;
    Ok(())
}
