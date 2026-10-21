//! Dark Age buildings imported from the owner's Age of Empires II: Definitive
//! Edition install into a separate local pack. The install stays outside Git;
//! the pack, the preview PNGs and the mapping table all go below the ignored
//! `local-assets/`. The SLD layers become the same colour, player-ramp and
//! shadow planes the trial packs use, so the client reads both alike.
use crate::{
    Error,
    pack::{
        self,
        atlas::{SpriteRef, write_pack},
    },
    sld::{self, Frame},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

mod compose;
mod farm;
mod preview;
mod select;
mod table;

pub use preview::reference;
pub use select::{FARM_TEXTURES, Item, SELECTION};

const MAX_SLD: u64 = 64 * 1024 * 1024;
const GRAPHICS: &str = "resources/_common/drs/graphics";
const TEXTURES: &str = "resources/_common/terrain/textures/2x";
/// Folded into the pack hash so a change of the conversion rules makes a new pack.
const CONVERSION: &str = "de-sld-rgb565-weighted-player";

/// What one selected source contributed to the pack.
#[derive(Clone, Debug)]
pub struct Entry {
    pub building: &'static str,
    pub role: &'static str,
    /// Frame `source` id in the pack manifest.
    pub source: String,
    pub frames: Vec<FrameInfo>,
}

#[derive(Clone, Debug)]
pub struct FrameInfo {
    /// Pack frame size after cropping to the drawn layers.
    pub width: u16,
    pub height: u16,
    pub anchor: (i32, i32),
    /// The frame canvas declared by the file.
    pub canvas: (u16, u16),
    /// Size of the main (colour) layer alone, which is the building body.
    pub body: (u16, u16),
}

#[derive(Debug)]
pub struct Report {
    pub pack: PathBuf,
    pub entries: Vec<Entry>,
    /// Selected sources the install does not have (building, role, file).
    pub absent: Vec<(&'static str, &'static str, String)>,
}

struct Loaded {
    item: &'static Item,
    source: String,
    hash: String,
    frames: Vec<Frame>,
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, Error> {
    if fs::metadata(path)?.len() > MAX_SLD {
        return Err(crate::invalid(
            "DE",
            0,
            format!("{} is too large", path.display()),
        ));
    }
    Ok(fs::read(path)?)
}

/// Imports the selection from `install` (the game's root directory) into a pack
/// below `output_root` (`local-assets/packs`), and writes previews and the
/// mapping table under `local-assets/`.
pub fn import(install: &Path, output_root: &Path) -> Result<Report, Error> {
    let output_root = pack::output_path(output_root)?;
    let local = output_root
        .parent()
        .ok_or_else(|| crate::invalid("DE", 0, "output root has no parent"))?
        .to_path_buf();
    let install = install.canonicalize()?;
    if install.starts_with(&local) || local.starts_with(&install) {
        return Err(crate::invalid("DE", 0, "install and output paths overlap"));
    }
    let graphics = install.join(GRAPHICS);
    if !graphics.is_dir() {
        return Err(crate::invalid(
            "DE",
            0,
            format!("{} is not a Definitive Edition install", install.display()),
        ));
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"pack indexed building startup data\0");
    hasher.update(CONVERSION.as_bytes());
    let mut loaded = Vec::new();
    let mut absent = Vec::new();
    for item in SELECTION {
        let file = format!("{}_x1.sld", item.stem);
        let path = graphics.join(&file);
        if !path.is_file() {
            absent.push((item.building, item.role, file));
            continue;
        }
        let bytes = read_bounded(&path)?;
        let hash = blake3::hash(&bytes);
        hasher.update(file.as_bytes());
        hasher.update(hash.as_bytes());
        let frames = sld::parse(&bytes)?;
        if frames.is_empty() {
            return Err(crate::invalid(
                "DE",
                6,
                format!("{} has zero frames", path.display()),
            ));
        }
        loaded.push(Loaded {
            item,
            source: format!("de/{file}"),
            hash: hash.to_hex().to_string(),
            frames,
        });
    }
    let farm = farm::load(&install.join(TEXTURES), &mut hasher)?;
    let mut refs: Vec<SpriteRef> = loaded
        .iter()
        .map(|sprite| SpriteRef {
            id: sprite.source.clone(),
            hash: sprite.hash.clone(),
            frames: sprite.frames.len(),
        })
        .collect();
    refs.extend(farm.iter().map(farm::Sheet::sprite));
    let split = loaded.len();
    let mut entries = Vec::new();
    for sprite in &loaded {
        entries.push(Entry {
            building: sprite.item.building,
            role: sprite.item.role,
            source: sprite.source.clone(),
            frames: sprite.frames.iter().map(compose::info).collect(),
        });
    }
    let pack = write_pack(
        &output_root,
        hasher.finalize().to_hex().to_string(),
        &refs,
        Vec::new(),
        |sprite, frame| {
            if sprite < split {
                Ok(compose::planes(&loaded[sprite].frames[frame]))
            } else {
                farm[sprite - split].planes(frame)
            }
        },
    )?;
    let report = Report {
        pack,
        entries,
        absent,
    };
    preview::write(&local.join("de-preview"), &loaded_frames(&loaded), &farm)?;
    table::write(&local.join("de-table.md"), &report, &farm)?;
    Ok(report)
}

fn loaded_frames(loaded: &[Loaded]) -> Vec<(&str, &[Frame])> {
    loaded
        .iter()
        .map(|sprite| (sprite.item.stem, sprite.frames.as_slice()))
        .collect()
}
