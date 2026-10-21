//! Deterministic, content-addressed local PNG atlas packs.
use crate::{
    Error, drs, invalid,
    palette::{self, Palette},
    slp::{self, Frame},
};
use atlas::{Planes, SpriteRef, write_pack};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};
use walkdir::WalkDir;

const PAGE: usize = 2_048;
const MAX_INPUT: u64 = 512 * 1024 * 1024;
const MAX_FRAMES: usize = 25_000;
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
    pub buildings: Vec<crate::dat::BuildingComposition>,
    pub building_data: String,
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
    hasher.update(b"AoK DAT building graphics, angles, indexed recipes and foundation topology\0");
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

pub(crate) fn output_path(root: &Path) -> Result<PathBuf, Error> {
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
    let refs: Vec<SpriteRef> = sprites
        .iter()
        .map(|sprite| SpriteRef {
            id: sprite.id.clone(),
            hash: sprite.hash.clone(),
            frames: sprite.frames.len(),
        })
        .collect();
    let mut buildings = match inputs
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case("empires2.dat"))
    {
        Some(input) => {
            let dat = crate::dat::decode(&input.bytes)?;
            if let Some(reason) = &dat.report.stopped {
                return Err(invalid("composition", 0, reason));
            }
            crate::dat::table::DARK_AGE
                .iter()
                .chain(crate::dat::table::FEUDAL)
                .filter(|(_, id)| *id != 618)
                .filter(|(_, id)| dat.unit(*id).is_some_and(|u| u.building.is_some()))
                .map(|(_, id)| dat.building_composition(*id))
                .collect::<Result<Vec<_>, _>>()?
        }
        None => Vec::new(),
    };
    for building in &mut buildings {
        for part in building.parts.iter_mut().chain(&mut building.construction) {
            part.available = sprites.iter().any(|s| s.id == part.source);
        }
    }
    let mut identity = blake3::Hasher::new();
    identity.update(hash.as_bytes());
    identity.update(&serde_json::to_vec(&buildings)?);
    let hash = identity.finalize().to_hex().to_string();
    write_pack(&output_root, hash, &refs, buildings, |sprite, frame| {
        Planes::from_slp(&sprites[sprite].frames[frame], &palette)
    })
}

pub(crate) mod atlas;
mod player;
pub use player::blend_player;
pub mod composite;
pub mod recipes;
mod verify;
pub use verify::{parse_manifest, verify};

#[path = "pack/tests.rs"]
#[cfg(test)]
mod tests;
