//! Generic DAT units and buildings from the owner-licensed DE installation.
//! Native pixels remain private; the runtime streams bounded visible cells.
use crate::{
    Error,
    pack::{
        self,
        atlas::{SpriteRef, write_pack_streamed},
    },
    sld,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub mod buildings;
mod cache;
mod compose;
mod native;
mod preview;
mod select;
mod table;
pub mod terrain;
pub mod unit_catalog;
mod unit_frames;

pub use preview::reference;
pub use select::{Item, SELECTION};

const MAX_SLD: u64 = 64 * 1024 * 1024;
const GRAPHICS: &str = "resources/_common/drs/graphics";
const TEXTURES: &str = "resources/_common/terrain/textures/2x";
/// Folded into the pack hash so a change of the conversion rules makes a new pack.
const CONVERSION: &str = "de-sld-rgb565-weighted-player-chunked-dat-environment";

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
    building: &'static str,
    role: &'static str,
    stem: String,
    source: String,
    hash: String,
    path: PathBuf,
    frames: Vec<FrameInfo>,
    unit: Option<unit_catalog::Selected>,
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
    let units = unit_catalog::resolve(&install)?;
    let buildings = buildings::resolve(&install)?;
    native::validate_catalog(&units)?;
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
    hasher.update(&serde_json::to_vec(&buildings.recipes)?);
    let mut loaded = Vec::new();
    let mut absent = Vec::new();
    let mut plans: Vec<_> = SELECTION
        .iter()
        .map(|i| (format!("{}_x1", i.stem), i.building, i.role))
        .collect();
    plans.extend(buildings.sources.iter().map(|s| {
        (
            s.trim_start_matches("de/")
                .trim_end_matches(".sld")
                .to_owned(),
            "building",
            "native",
        )
    }));
    for (stem, building, role) in plans {
        let file = format!("{stem}.sld");
        let path = graphics.join(&file);
        if !path.is_file() {
            if matches!(building, "unit" | "projectile") {
                return Err(crate::invalid("DE actor", 0, format!("missing {file}")));
            }
            absent.push((building, role, file));
            continue;
        }
        let bytes = read_bounded(&path)?;
        let hash = blake3::hash(&bytes);
        hasher.update(file.as_bytes());
        hasher.update(hash.as_bytes());
        let mut decoder = sld::Stream::new(bytes)?;
        let frames = (0..decoder.len())
            .map(|i| decoder.frame(i).map(compose::info))
            .collect::<Result<Vec<_>, Error>>()?;
        if let Some(source) = crate::catalog::units::SPRITES
            .iter()
            .find(|source| source.is_native() && source.manifest_source() == format!("de/{file}"))
            && frames.len() != source.frames() as usize
        {
            return Err(crate::invalid(
                "DE actor",
                0,
                "unsupported native actor topology",
            ));
        }
        if building == "projectile" && frames.len() != 30 {
            return Err(crate::invalid(
                "DE projectile",
                0,
                "unsupported native stone topology",
            ));
        }
        if frames.is_empty() {
            return Err(crate::invalid(
                "DE",
                6,
                format!("{} has zero frames", path.display()),
            ));
        }
        loaded.push(Loaded {
            building,
            role,
            stem,
            source: format!("de/{file}"),
            hash: hash.to_hex().to_string(),
            path,
            frames,
            unit: None,
        });
    }
    let unique: std::collections::BTreeMap<_, _> =
        units.into_iter().map(|u| (u.sprite, u)).collect();
    for (_, unit) in unique {
        let hash = unit_frames::hash(&install, &unit)?;
        hasher.update(hash.as_bytes());
        let mut decoder = unit_frames::Decoder::new(&install, &unit)?;
        let frames = (0..decoder.len())
            .map(|i| decoder.frame(i))
            .map(|p| {
                p.map(|p| FrameInfo {
                    width: p.width,
                    height: p.height,
                    anchor: (p.hotspot_x, p.hotspot_y),
                    canvas: (p.width, p.height),
                    body: (p.width, p.height),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        loaded.push(Loaded {
            building: "unit",
            role: "native",
            stem: unit.file.clone(),
            source: format!("de/{}.sld", unit.file),
            hash,
            path: PathBuf::new(),
            frames,
            unit: Some(unit),
        });
    }
    for b in &buildings.recipes {
        for p in b
            .parts
            .iter()
            .chain(&b.construction)
            .chain(b.states.iter().flat_map(|s| &s.parts))
            .chain(b.damage.iter().flat_map(|d| &d.parts))
            .filter(|p| p.available)
        {
            let count = loaded
                .iter()
                .find(|s| s.source == p.source)
                .map_or(0, |s| s.frames.len());
            if count == 0
                || (count < usize::from(p.frames) * usize::from(p.angles)
                    && !(p.frames == 1 && p.frame_rate == 0.0))
            {
                return Err(crate::invalid(
                    "DE composition",
                    0,
                    "incomplete native animation rows",
                ));
            }
        }
    }
    let terrain = terrain::resolve(&install, &mut hasher)?;
    let mut refs: Vec<SpriteRef> = loaded
        .iter()
        .map(|sprite| SpriteRef {
            id: sprite.source.clone(),
            hash: sprite.hash.clone(),
            frames: sprite.frames.len(),
        })
        .collect();
    refs.extend(terrain.sheets.iter().map(|s| SpriteRef {
        id: s.source.clone(),
        hash: s.hash.clone(),
        frames: s.frames,
    }));
    let split = loaded.len();
    let mut entries = Vec::new();
    for sprite in &loaded {
        entries.push(Entry {
            building: sprite.building,
            role: sprite.role,
            source: sprite.source.clone(),
            frames: sprite.frames.clone(),
        });
    }
    let mut cache = cache::Cache::new(|index: usize| {
        let sprite = &loaded[index];
        if let Some(unit) = &sprite.unit {
            if unit_frames::hash(&install, unit)? != sprite.hash {
                return Err(crate::invalid("DE unit", 0, "source changed during import"));
            }
            Ok(vec![unit_frames::Decoder::new(&install, unit)?])
        } else {
            let bytes = read_bounded(&sprite.path)?;
            if blake3::hash(&bytes).to_hex().as_str() != sprite.hash {
                return Err(crate::invalid("DE", 0, "source changed during import"));
            }
            Ok(vec![unit_frames::Decoder::streamed(bytes)?])
        }
    });
    let mut terrain_cache = cache::Cache::new(|index: usize| {
        let s = &terrain.sheets[index];
        let bytes = read_bounded(&s.path)?;
        if blake3::hash(&bytes).to_hex().as_str() != s.hash {
            return Err(crate::invalid(
                "DE terrain",
                0,
                "source changed during import",
            ));
        }
        Ok(vec![terrain::Decoder::new(crate::dds::parse(&bytes)?)])
    });
    let pack = write_pack_streamed(
        &output_root,
        hasher.finalize().to_hex().to_string(),
        &refs,
        buildings.recipes,
        terrain.terrains,
        |sprite, _| {
            sprite >= split
                || (sprite < split
                    && matches!(loaded[sprite].building, "unit" | "projectile" | "building"))
        },
        |sprite, frame| {
            if sprite < split {
                cache.get(sprite)?[0].frame(frame)
            } else {
                terrain_cache.get(sprite - split)?[0].frame(frame)
            }
        },
    )?;
    let report = Report {
        pack,
        entries,
        absent,
    };
    for sprite in loaded
        .iter()
        .filter(|s| s.unit.is_none() && s.building != "building")
    {
        preview::write(
            &local.join("de-preview"),
            &[(
                sprite.stem.strip_suffix("_x1").unwrap_or(&sprite.stem),
                sld::parse(&read_bounded(&sprite.path)?)?.as_slice(),
            )],
        )?;
    }
    table::write(&local.join("de-table.md"), &report)?;
    Ok(report)
}
