//! DOM-ready UI pages, retaining every original colour and mask page unchanged.
use super::{Manifest, blend_player};
use crate::{
    Error,
    catalog::interface::{Art, Interface},
    invalid,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufWriter, Cursor},
    path::Path,
};

pub(super) fn compose_pixel(color: [u8; 4], player: [u8; 4], shadow: u8) -> [u8; 4] {
    let body = blend_player(color, player[0], player[3], [65, 145, 245]);
    let alpha = u32::from(body[3]) + (u32::from(shadow) * u32::from(255 - body[3]) + 127) / 255;
    if alpha == 0 {
        return [0; 4];
    }
    let mut out = [0; 4];
    for (output, channel) in out[..3].iter_mut().zip(body) {
        *output = ((u32::from(channel) * u32::from(body[3]) + alpha / 2) / alpha) as u8;
    }
    out[3] = alpha as u8;
    out
}

fn read(path: &Path, width: u16, height: u16) -> Result<Vec<u8>, Error> {
    let data = fs::read(path)?;
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info()?;
    if reader.info().width != u32::from(width)
        || reader.info().height != u32::from(height)
        || reader.info().color_type != png::ColorType::Rgba
        || reader.info().bit_depth != png::BitDepth::Eight
    {
        return Err(invalid("interface", 0, "expected bounded RGBA8 page"));
    }
    let mut pixels = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut pixels)?;
    Ok(pixels)
}

fn arts(catalog: &mut Interface) -> impl Iterator<Item = &mut Art> {
    catalog
        .units
        .values_mut()
        .chain(catalog.technologies.values_mut())
        .chain(catalog.commands.values_mut())
        .chain(catalog.resources.values_mut())
        .chain(catalog.panels.values_mut())
}

pub(super) fn write(
    directory: &Path,
    manifest: &Manifest,
    catalog: &mut Interface,
) -> Result<(), Error> {
    let mut pages = BTreeMap::new();
    for art in arts(catalog) {
        pages.insert(art.page.clone(), String::new());
    }
    for (name, output) in &mut pages {
        let (index, page) = manifest
            .pages
            .iter()
            .enumerate()
            .find(|(_, p)| &p.color == name)
            .ok_or_else(|| invalid("interface", 0, "catalog page missing"))?;
        let color = read(&directory.join(&page.color), page.width, page.height)?;
        let player = read(&directory.join(&page.player), page.width, page.height)?;
        let shadow = read(&directory.join(&page.shadow), page.width, page.height)?;
        let mut pixels = Vec::with_capacity(color.len());
        let (colors, _) = color.as_chunks::<4>();
        let (players, _) = player.as_chunks::<4>();
        let (shadows, _) = shadow.as_chunks::<4>();
        for ((c, p), s) in colors.iter().zip(players).zip(shadows) {
            pixels.extend_from_slice(&compose_pixel(
                [c[0], c[1], c[2], c[3]],
                [p[0], p[1], p[2], p[3]],
                s[3],
            ));
        }
        *output = format!("interface-{index:03}.png");
        let temporary = directory.join(format!("interface-{index:03}-{}.tmp", std::process::id()));
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut encoder = png::Encoder::new(
            BufWriter::new(file),
            u32::from(page.width),
            u32::from(page.height),
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&pixels)?;
        drop(writer);
        fs::rename(temporary, directory.join(&*output))?;
    }
    for art in arts(catalog) {
        art.page = pages
            .get(&art.page)
            .ok_or_else(|| invalid("interface", 0, "page remap missing"))?
            .clone();
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/interface.rs"]
mod tests;
