//! Atlas page packing and the pack writer shared by every importer.
use super::{AtlasPage, FrameRecord, MAX_FRAMES, MAX_PAGES, Manifest, PAGE, verify};
use crate::{
    Error, invalid,
    palette::Palette,
    slp::{Frame, Pixel},
};
use std::{
    fs,
    io::BufWriter,
    path::{Path, PathBuf},
};

pub(super) struct PageData {
    pub(super) color: Vec<u8>,
    pub(super) player: Vec<u8>,
    pub(super) shadow: Vec<u8>,
    pub(super) outline: Vec<u8>,
    x: usize,
    y: usize,
    row_height: usize,
}

impl PageData {
    pub(super) fn new() -> Self {
        let bytes = PAGE * PAGE * 4;
        Self {
            color: vec![0; bytes],
            player: vec![0; bytes],
            shadow: vec![0; bytes],
            outline: vec![0; bytes],
            x: 1,
            y: 1,
            row_height: 0,
        }
    }

    #[cfg(test)]
    pub(super) fn place(
        &mut self,
        frame: &Frame,
        palette: &Palette,
    ) -> Result<Option<(usize, usize)>, Error> {
        self.place_planes(&Planes::from_slp(frame, palette)?)
    }

    pub(super) fn place_planes(&mut self, frame: &Planes) -> Result<Option<(usize, usize)>, Error> {
        let width = frame.width as usize;
        let height = frame.height as usize;
        if width + 2 > PAGE || height + 2 > PAGE {
            return Err(invalid("atlas", 0, "frame exceeds page size"));
        }
        if self.x + width + 1 > PAGE {
            self.x = 1;
            self.y += self.row_height + 1;
            self.row_height = 0;
        }
        if self.y + height + 1 > PAGE {
            return Ok(None);
        }
        let x = self.x;
        let y = self.y;
        for row in 0..height {
            let from = row * width * 4..(row + 1) * width * 4;
            let to = ((y + row) * PAGE + x) * 4;
            let to = to..to + width * 4;
            self.color[to.clone()].copy_from_slice(&frame.color[from.clone()]);
            self.player[to.clone()].copy_from_slice(&frame.player[from.clone()]);
            self.shadow[to.clone()].copy_from_slice(&frame.shadow[from.clone()]);
            self.outline[to].copy_from_slice(&frame.outline[from]);
        }
        self.x += width + 1;
        self.row_height = self.row_height.max(height);
        Ok(Some((x, y)))
    }
}

/// One frame as four RGBA planes (colour, player ramp, shadow, outline), the
/// form every importer hands to the atlas writer.
pub(crate) struct Planes {
    pub width: u16,
    pub height: u16,
    pub hotspot_x: i32,
    pub hotspot_y: i32,
    pub color: Vec<u8>,
    pub player: Vec<u8>,
    pub shadow: Vec<u8>,
    pub outline: Vec<u8>,
}

impl Planes {
    pub(crate) fn blank(width: u16, height: u16, hotspot_x: i32, hotspot_y: i32) -> Self {
        let bytes = usize::from(width) * usize::from(height) * 4;
        Self {
            width,
            height,
            hotspot_x,
            hotspot_y,
            color: vec![0; bytes],
            player: vec![0; bytes],
            shadow: vec![0; bytes],
            outline: vec![0; bytes],
        }
    }

    pub(super) fn from_slp(frame: &Frame, palette: &Palette) -> Result<Self, Error> {
        let mut planes = Self::blank(frame.width, frame.height, frame.hotspot_x, frame.hotspot_y);
        for (index, pixel) in frame.pixels.iter().enumerate() {
            let at = index * 4..index * 4 + 4;
            match *pixel {
                Pixel::Transparent => {}
                Pixel::Color(value) => {
                    let color = palette.0.get(value as usize).ok_or_else(|| {
                        invalid("atlas", 0, format!("palette index {value} missing"))
                    })?;
                    planes.color[at].copy_from_slice(&[color[0], color[1], color[2], 255]);
                }
                Pixel::Player(value) => planes.player[at].copy_from_slice(&[value, 0, 0, 255]),
                Pixel::Shadow => planes.shadow[at].copy_from_slice(&[0, 0, 0, 128]),
                Pixel::Outline1 => planes.outline[at].copy_from_slice(&[1, 0, 0, 255]),
                Pixel::Outline2 => planes.outline[at].copy_from_slice(&[2, 0, 0, 255]),
            }
        }
        Ok(planes)
    }
}

fn write_png(path: &Path, pixels: &[u8]) -> Result<String, Error> {
    let file = fs::File::create(path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), PAGE as u32, PAGE as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(pixels)?;
    drop(writer);
    Ok(blake3::hash(&fs::read(path)?).to_hex().to_string())
}

pub(super) fn save_page(
    directory: &Path,
    index: usize,
    page: PageData,
) -> Result<AtlasPage, Error> {
    let names = [
        format!("color-{index:03}.png"),
        format!("player-{index:03}.png"),
        format!("shadow-{index:03}.png"),
        format!("outline-{index:03}.png"),
    ];
    let hashes = [
        write_png(&directory.join(&names[0]), &page.color)?,
        write_png(&directory.join(&names[1]), &page.player)?,
        write_png(&directory.join(&names[2]), &page.shadow)?,
        write_png(&directory.join(&names[3]), &page.outline)?,
    ];
    Ok(AtlasPage {
        color: names[0].clone(),
        color_hash: hashes[0].clone(),
        player: names[1].clone(),
        player_hash: hashes[1].clone(),
        shadow: names[2].clone(),
        shadow_hash: hashes[2].clone(),
        outline: names[3].clone(),
        outline_hash: hashes[3].clone(),
        width: PAGE as u16,
        height: PAGE as u16,
    })
}

/// One sprite of a pack: its identity and how many frames `write_pack` asks for.
pub(crate) struct SpriteRef {
    pub id: String,
    pub hash: String,
    pub frames: usize,
}

/// Packs `render(sprite, frame)` for every listed frame into a content-addressed
/// pack below `output_root`, verifying it before publishing. Frames are rendered
/// one at a time so only one is resident beside the atlas page.
pub(crate) fn write_pack(
    output_root: &Path,
    hash: String,
    sprites: &[SpriteRef],
    buildings: Vec<crate::dat::BuildingComposition>,
    mut render: impl FnMut(usize, usize) -> Result<Planes, Error>,
) -> Result<PathBuf, Error> {
    let total = sprites.iter().try_fold(0_usize, |sum, sprite| {
        sum.checked_add(sprite.frames).filter(|&n| n <= MAX_FRAMES)
    });
    if total.is_none() {
        return Err(invalid("atlas", 0, "too many sprite frames"));
    }
    fs::create_dir_all(output_root)?;
    let destination = output_root.join(&hash);
    if destination.exists() {
        verify(&destination)?;
        return Ok(destination);
    }
    let temporary = output_root.join(format!(".tmp-{hash}-{}", std::process::id()));
    fs::create_dir(&temporary)?;
    let result = (|| {
        let mut page = PageData::new();
        let mut pages = Vec::new();
        let mut frames = Vec::new();
        for (sprite_index, sprite) in sprites.iter().enumerate() {
            for frame_index in 0..sprite.frames {
                let frame = render(sprite_index, frame_index)?;
                let mut location = page.place_planes(&frame)?;
                if location.is_none() {
                    if pages.len() + 1 == MAX_PAGES {
                        return Err(invalid("atlas", 0, "too many atlas pages"));
                    }
                    pages.push(save_page(&temporary, pages.len(), page)?);
                    page = PageData::new();
                    location = page.place_planes(&frame)?;
                }
                let (x, y) = location
                    .ok_or_else(|| invalid("atlas", 0, "frame does not fit an empty page"))?;
                frames.push(FrameRecord {
                    source: sprite.id.clone(),
                    source_hash: sprite.hash.clone(),
                    frame: frame_index as u32,
                    page: pages.len() as u16,
                    x: x as u16,
                    y: y as u16,
                    width: frame.width,
                    height: frame.height,
                    anchor_x: frame.hotspot_x,
                    anchor_y: frame.hotspot_y,
                });
            }
        }
        pages.push(save_page(&temporary, pages.len(), page)?);
        let manifest = Manifest {
            building_data: super::recipes::encode(&buildings, &frames)?,
            version: 1,
            converter: env!("CARGO_PKG_VERSION").to_owned(),
            input_hash: hash,
            pages,
            frames,
            buildings,
        };
        fs::write(
            temporary.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        verify(&temporary)?;
        fs::rename(&temporary, &destination)?;
        Ok(destination)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}
