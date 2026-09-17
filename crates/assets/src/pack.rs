//! Deterministic, content-addressed local PNG atlas packs.
use crate::{
    Error, drs, invalid,
    palette::{self, Palette},
    slp::{self, Frame, Pixel},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::BufWriter,
    path::{Component, Path, PathBuf},
};
use walkdir::WalkDir;

const PAGE: usize = 1_024;
const MAX_INPUT: u64 = 512 * 1024 * 1024;
const MAX_FRAMES: usize = 20_000;
const MAX_PAGES: usize = 256;

#[derive(Clone, Debug, Serialize)]
pub struct Inventory {
    pub drs: usize,
    pub slp: usize,
    pub palettes: usize,
    pub unsupported: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u16,
    pub converter: String,
    pub input_hash: String,
    pub pages: Vec<AtlasPage>,
    pub frames: Vec<FrameRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AtlasPage {
    pub color: String,
    pub color_hash: String,
    pub player: String,
    pub player_hash: String,
    pub shadow: String,
    pub shadow_hash: String,
    pub outline: String,
    pub outline_hash: String,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrameRecord {
    pub source: String,
    pub source_hash: String,
    pub frame: u32,
    pub page: u16,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub anchor_x: i32,
    pub anchor_y: i32,
}

struct Input {
    name: String,
    bytes: Vec<u8>,
}

struct SpriteSource {
    id: String,
    hash: String,
    frames: Vec<Frame>,
}

struct PageData {
    color: Vec<u8>,
    player: Vec<u8>,
    shadow: Vec<u8>,
    outline: Vec<u8>,
    x: usize,
    y: usize,
    row_height: usize,
}

impl PageData {
    fn new() -> Self {
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

    fn place(&mut self, frame: &Frame, palette: &Palette) -> Result<Option<(usize, usize)>, Error> {
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
            for col in 0..width {
                let index = ((y + row) * PAGE + x + col) * 4;
                match frame.pixels[row * width + col] {
                    Pixel::Transparent => {}
                    Pixel::Color(value) => {
                        let color = palette.0.get(value as usize).ok_or_else(|| {
                            invalid("atlas", 0, format!("palette index {value} missing"))
                        })?;
                        self.color[index..index + 4]
                            .copy_from_slice(&[color[0], color[1], color[2], 255]);
                    }
                    Pixel::Player(value) => {
                        self.player[index..index + 4].copy_from_slice(&[value, 0, 0, 255])
                    }
                    Pixel::Shadow => self.shadow[index..index + 4].copy_from_slice(&[0, 0, 0, 128]),
                    Pixel::Outline1 => {
                        self.outline[index..index + 4].copy_from_slice(&[1, 0, 0, 255])
                    }
                    Pixel::Outline2 => {
                        self.outline[index..index + 4].copy_from_slice(&[2, 0, 0, 255])
                    }
                }
            }
        }
        self.x += width + 1;
        self.row_height = self.row_height.max(height);
        Ok(Some((x, y)))
    }
}

fn files(root: &Path) -> Result<Vec<Input>, Error> {
    if !root.is_dir() {
        return Err(invalid(
            "assets",
            0,
            format!("input directory missing: {}", root.display()),
        ));
    }
    let mut inputs = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|e| invalid("assets", 0, e.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| invalid("assets", 0, "input path escaped root"))?
            .to_string_lossy()
            .to_string();
        let metadata = entry
            .metadata()
            .map_err(|e| invalid("assets", 0, e.to_string()))?;
        if metadata.len() > MAX_INPUT {
            return Err(invalid("assets", 0, format!("{name} exceeds input limit")));
        }
        inputs.push(Input {
            name,
            bytes: fs::read(entry.path())?,
        });
    }
    inputs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(inputs)
}

pub fn inspect(root: &Path) -> Result<Inventory, Error> {
    let mut inventory = Inventory {
        drs: 0,
        slp: 0,
        palettes: 0,
        unsupported: Vec::new(),
    };
    for input in files(root)? {
        let lower = input.name.to_ascii_lowercase();
        if lower.ends_with(".drs") {
            for entry in drs::parse(&input.bytes)? {
                if entry.kind == *b" pls" {
                    slp::parse(entry.data)?;
                    inventory.slp += 1;
                } else if entry.data.starts_with(b"JASC-PAL") {
                    palette::parse(entry.data)?;
                    inventory.palettes += 1;
                } else {
                    inventory
                        .unsupported
                        .push(format!("{}:{:?}:{}", input.name, entry.kind, entry.id));
                }
            }
            inventory.drs += 1;
        } else if lower.ends_with(".slp") {
            slp::parse(&input.bytes)?;
            inventory.slp += 1;
        } else if input.bytes.starts_with(b"JASC-PAL") {
            palette::parse(&input.bytes)?;
            inventory.palettes += 1;
        } else {
            inventory.unsupported.push(input.name);
        }
    }
    Ok(inventory)
}

fn collect(inputs: &[Input]) -> Result<(Palette, Vec<SpriteSource>, String), Error> {
    let mut hasher = blake3::Hasher::new();
    let mut palette_data = None;
    let mut sprites = Vec::new();
    let mut ids = BTreeSet::new();
    for input in inputs {
        hasher.update(&(input.name.len() as u64).to_le_bytes());
        hasher.update(input.name.as_bytes());
        hasher.update(blake3::hash(&input.bytes).as_bytes());
        let lower = input.name.to_ascii_lowercase();
        if lower.ends_with(".drs") {
            for entry in drs::parse(&input.bytes)? {
                let id = format!("{}:{:?}:{}", input.name, entry.kind, entry.id);
                let hash = blake3::hash(entry.data).to_hex().to_string();
                if entry.kind == *b" pls" {
                    if !ids.insert((entry.kind, entry.id)) {
                        return Err(invalid(
                            "assets",
                            0,
                            format!("conflicting resource ID: {}", entry.id),
                        ));
                    }
                    sprites.push(SpriteSource {
                        id,
                        hash,
                        frames: slp::parse(entry.data)?,
                    });
                } else if entry.data.starts_with(b"JASC-PAL") && entry.id == 50500 {
                    if !ids.insert((entry.kind, entry.id)) {
                        return Err(invalid("assets", 0, "conflicting palette 50500"));
                    }
                    palette_data = Some(palette::parse(entry.data)?);
                }
            }
        } else if lower.ends_with(".slp") {
            sprites.push(SpriteSource {
                id: input.name.clone(),
                hash: blake3::hash(&input.bytes).to_hex().to_string(),
                frames: slp::parse(&input.bytes)?,
            });
        } else if input.bytes.starts_with(b"JASC-PAL") {
            if palette_data.is_some() {
                return Err(invalid("assets", 0, "conflicting palettes"));
            }
            palette_data = Some(palette::parse(&input.bytes)?);
        }
    }
    sprites.sort_by(|a, b| a.id.cmp(&b.id));
    let palette = palette_data.ok_or_else(|| invalid("assets", 0, "JASC palette 50500 missing"))?;
    let total_frames: usize = sprites.iter().map(|sprite| sprite.frames.len()).sum();
    if total_frames > MAX_FRAMES {
        return Err(invalid("assets", 0, "too many sprite frames"));
    }
    Ok((palette, sprites, hasher.finalize().to_hex().to_string()))
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

fn save_pages(directory: &Path, pages: Vec<PageData>) -> Result<Vec<AtlasPage>, Error> {
    let mut records = Vec::new();
    for (index, page) in pages.into_iter().enumerate() {
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
        records.push(AtlasPage {
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
        });
    }
    Ok(records)
}

fn output_path(root: &Path) -> Result<PathBuf, Error> {
    let cwd = std::env::current_dir()?.canonicalize()?;
    let relative = if root.is_absolute() {
        root.strip_prefix(&cwd)
            .map_err(|_| invalid("assets", 0, "output must be inside checkout"))?
    } else {
        root
    };
    let components: Vec<_> = relative.components().collect();
    if components.len() != 2
        || components[0] != Component::Normal(std::ffi::OsStr::new("local-assets"))
        || components[1] != Component::Normal(std::ffi::OsStr::new("packs"))
    {
        return Err(invalid("assets", 0, "output must be local-assets/packs"));
    }
    let local = cwd.join("local-assets");
    if local.exists() && fs::symlink_metadata(&local)?.file_type().is_symlink() {
        return Err(invalid("assets", 0, "local-assets symlink is not allowed"));
    }
    let packs = local.join("packs");
    if packs.exists() && fs::symlink_metadata(&packs)?.file_type().is_symlink() {
        return Err(invalid("assets", 0, "packs symlink is not allowed"));
    }
    Ok(cwd.join(relative))
}

pub fn import(input: &Path, output_root: &Path) -> Result<PathBuf, Error> {
    let output_root = output_path(output_root)?;
    let input = input.canonicalize()?;
    if input.starts_with(&output_root) || output_root.starts_with(&input) {
        return Err(invalid("assets", 0, "input and output paths overlap"));
    }
    let inputs = files(&input)?;
    let (palette, sprites, hash) = collect(&inputs)?;
    if sprites.is_empty() {
        return Err(invalid("assets", 0, "no supported sprites found"));
    }
    fs::create_dir_all(&output_root)?;
    let destination = output_root.join(&hash);
    if destination.exists() {
        verify(&destination)?;
        return Ok(destination);
    }
    let temporary = output_root.join(format!(".tmp-{hash}-{}", std::process::id()));
    fs::create_dir(&temporary)?;
    let result = (|| {
        let mut pages = vec![PageData::new()];
        let mut frames = Vec::new();
        for sprite in sprites {
            for (frame_index, frame) in sprite.frames.iter().enumerate() {
                let mut location = pages
                    .last_mut()
                    .ok_or_else(|| invalid("atlas", 0, "no page"))?
                    .place(frame, &palette)?;
                if location.is_none() {
                    if pages.len() == MAX_PAGES {
                        return Err(invalid("atlas", 0, "too many atlas pages"));
                    }
                    pages.push(PageData::new());
                    location = pages
                        .last_mut()
                        .ok_or_else(|| invalid("atlas", 0, "no page"))?
                        .place(frame, &palette)?;
                }
                let (x, y) = location
                    .ok_or_else(|| invalid("atlas", 0, "frame does not fit an empty page"))?;
                frames.push(FrameRecord {
                    source: sprite.id.clone(),
                    source_hash: sprite.hash.clone(),
                    frame: frame_index as u32,
                    page: (pages.len() - 1) as u16,
                    x: x as u16,
                    y: y as u16,
                    width: frame.width,
                    height: frame.height,
                    anchor_x: frame.hotspot_x,
                    anchor_y: frame.hotspot_y,
                });
            }
        }
        let pages = save_pages(&temporary, pages)?;
        let manifest = Manifest {
            version: 1,
            converter: env!("CARGO_PKG_VERSION").to_owned(),
            input_hash: hash,
            pages,
            frames,
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

mod verify;
pub use verify::verify;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_masks_and_verifier_reject_tampering() {
        let temp = tempfile::tempdir().unwrap();
        let frame = Frame {
            width: 3,
            height: 1,
            hotspot_x: -1,
            hotspot_y: 0,
            pixels: vec![Pixel::Color(0), Pixel::Player(7), Pixel::Outline1],
        };
        let mut page = PageData::new();
        assert_eq!(
            page.place(&frame, &Palette(vec![[12, 34, 56]])).unwrap(),
            Some((1, 1))
        );
        let pixel = (PAGE + 1) * 4;
        assert_eq!(&page.color[pixel..pixel + 4], &[12, 34, 56, 255]);
        assert_eq!(&page.player[pixel + 4..pixel + 8], &[7, 0, 0, 255]);
        assert_eq!(&page.outline[pixel + 8..pixel + 12], &[1, 0, 0, 255]);
        let pages = save_pages(temp.path(), vec![page]).unwrap();
        let manifest = Manifest {
            version: 1,
            converter: "test".to_owned(),
            input_hash: "a".repeat(64),
            pages,
            frames: vec![FrameRecord {
                source: "fixture.slp".to_owned(),
                source_hash: "b".repeat(64),
                frame: 0,
                page: 0,
                x: 1,
                y: 1,
                width: 3,
                height: 1,
                anchor_x: -1,
                anchor_y: 0,
            }],
        };
        fs::write(
            temp.path().join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert_eq!(verify(temp.path()).unwrap().frames.len(), 1);
        fs::write(temp.path().join("color-000.png"), b"tampered").unwrap();
        assert!(verify(temp.path()).is_err());
    }

    #[test]
    fn invalid_output_path_is_rejected() {
        assert!(output_path(Path::new("../outside")).is_err());
        assert!(output_path(Path::new("local-assets/packs")).is_ok());
    }
}
