//! DAT icon indices select private DE DDS cells, packed and verified like other art.
use super::{read_bounded, unit_catalog};
use crate::{
    Error,
    catalog::interface::{Art, Interface},
    invalid,
    pack::{
        Manifest,
        atlas::{Planes, SpriteRef},
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
mod ui;

struct Source {
    icon: i16,
    technology: bool,
    path: PathBuf,
    hash: String,
}
pub(super) struct Icons {
    sources: Vec<Source>,
    ui: Vec<ui::Source>,
    units: Vec<(i16, i16)>,
    technologies: Vec<(i16, i16)>,
}

fn inventory(directory: &Path) -> Result<BTreeMap<i16, PathBuf>, Error> {
    named_inventory(directory, Some("50730"))
}

fn named_inventory(directory: &Path, sheet: Option<&str>) -> Result<BTreeMap<i16, PathBuf>, Error> {
    let mut out = BTreeMap::new();
    for (count, entry) in fs::read_dir(directory)?.enumerate() {
        if count >= 4096 {
            return Err(invalid("DE icons", 0, "inventory bound"));
        }
        let entry = entry?;
        let name = entry.file_name();
        let Some((number, suffix)) = name.to_str().and_then(|s| s.split_once('_')) else {
            continue;
        };
        let Some(stem) = suffix.get(..suffix.len().saturating_sub(4)) else {
            continue;
        };
        if !suffix.to_ascii_lowercase().ends_with(".dds")
            || sheet.is_some_and(|wanted| stem != wanted)
        {
            continue;
        }
        let Ok(icon) = number.parse::<i16>() else {
            continue;
        };
        if icon < 0 || !entry.file_type()?.is_file() {
            continue;
        }
        if out.insert(icon, entry.path()).is_some() {
            return Err(invalid("DE icons", 0, "ambiguous sheet index"));
        }
    }
    Ok(out)
}
fn source(icon: i16, technology: bool) -> String {
    format!(
        "de/interface/{}{icon}.dds",
        if technology { "tech/" } else { "" }
    )
}

pub(super) fn resolve(
    install: &Path,
    selected: &[unit_catalog::Selected],
    hash: &mut blake3::Hasher,
) -> Result<Icons, Error> {
    let wanted: BTreeSet<_> = unit_catalog::BINDINGS
        .iter()
        .filter(|b| matches!(b.action, unit_catalog::Action::Idle))
        .map(|b| b.unit)
        .collect();
    let units: BTreeMap<_, _> = selected
        .iter()
        .filter(|s| wanted.contains(&s.unit))
        .map(|s| (s.unit, s.icon))
        .collect();
    let files = inventory(&install.join("widgetui/textures/ingame/units"))?;
    let dat = read_bounded(&install.join("resources/_common/dat/empires2_x2_p1.dat"))?;
    let technologies =
        crate::dat::de_survey::technology_icons(&crate::dat::de_survey::inflate(&dat)?)?;
    let tech_files = named_inventory(&install.join("widgetui/textures/ingame/tech"), None)?;
    let mut sources = Vec::new();
    hash.update(b"DE DAT native unit interface icons\0");
    hash.update(&serde_json::to_vec(&units)?);
    hash.update(&serde_json::to_vec(&technologies)?);
    for (technology, files, wanted) in [
        (
            false,
            &files,
            units.values().copied().collect::<BTreeSet<_>>(),
        ),
        (
            true,
            &tech_files,
            technologies.iter().map(|&(_, icon)| icon).collect(),
        ),
    ] {
        for icon in wanted {
            let path = files
                .get(&icon)
                .ok_or_else(|| invalid("DE icons", 0, format!("missing sheet index {icon}")))?;
            let bytes = read_bounded(path)?;
            let digest = blake3::hash(&bytes);
            hash.update(digest.as_bytes());
            sources.push(Source {
                icon,
                technology,
                path: path.clone(),
                hash: digest.to_hex().to_string(),
            });
        }
    }
    let ui = ui::resolve(install, hash)?;
    Ok(Icons {
        sources,
        ui,
        units: units.into_iter().collect(),
        technologies,
    })
}
impl Icons {
    pub(super) fn refs(&self) -> Vec<SpriteRef> {
        self.sources
            .iter()
            .map(|s| SpriteRef {
                id: source(s.icon, s.technology),
                hash: s.hash.clone(),
                frames: 1,
            })
            .chain(self.ui.iter().map(|s| SpriteRef {
                id: s.id.clone(),
                hash: s.hash.clone(),
                frames: 1,
            }))
            .collect()
    }
    pub(super) fn frame(&self, index: usize) -> Result<Planes, Error> {
        if index >= self.sources.len() {
            let s = self
                .ui
                .get(index - self.sources.len())
                .ok_or_else(|| invalid("DE icons", 0, "source index"))?;
            let bytes = read_bounded(&s.path)?;
            if blake3::hash(&bytes).to_hex().as_str() != s.hash {
                return Err(invalid("DE icons", 0, "source changed during import"));
            }
            return ui::frame(s, &bytes);
        }
        let s = &self.sources[index];
        let bytes = read_bounded(&s.path)?;
        if blake3::hash(&bytes).to_hex().as_str() != s.hash {
            return Err(invalid("DE icons", 0, "source changed during import"));
        }
        decode(&bytes)
    }

    pub(super) fn write(&self, directory: &Path) -> Result<(), Error> {
        let manifest: Manifest =
            serde_json::from_slice(&read_bounded(&directory.join("manifest.json"))?)?;
        let mut icons = catalog(&manifest, &self.units, &self.technologies)?;
        ui::apply(&manifest, &self.ui, &mut icons)?;
        fs::write(
            directory.join("interface.json"),
            serde_json::to_vec_pretty(&icons)?,
        )?;
        Ok(())
    }
}
fn decode(bytes: &[u8]) -> Result<Planes, Error> {
    if crate::u32_at(bytes, 12, "DE icons")? > 256 || crate::u32_at(bytes, 16, "DE icons")? > 256 {
        return Err(invalid("DE icons", 0, "icon dimensions"));
    }
    let texture = crate::dds::parse(bytes)?;
    let mut planes = Planes::blank(texture.width, texture.height, 0, 0);
    planes.color = texture.pixels.into_iter().flatten().collect();
    Ok(planes)
}

fn catalog(
    manifest: &Manifest,
    units: &[(i16, i16)],
    technologies: &[(i16, i16)],
) -> Result<Interface, Error> {
    let mut out = Interface::default();
    for (technology, entries) in [(false, units), (true, technologies)] {
        for &(unit, icon) in entries {
            let id = source(icon, technology);
            let frame = manifest
                .frames
                .iter()
                .find(|f| f.source == id && f.frame == 0 && f.native.is_none())
                .ok_or_else(|| invalid("DE icons", 0, "missing icon cell"))?;
            let page = manifest
                .pages
                .get(usize::from(frame.page))
                .ok_or_else(|| invalid("DE icons", 0, "missing icon page"))?;
            let group = if technology {
                &mut out.technologies
            } else {
                &mut out.units
            };
            group.insert(
                i32::from(unit),
                Art {
                    page: page.color.clone(),
                    x: frame.x,
                    y: frame.y,
                    width: frame.width,
                    height: frame.height,
                    page_width: page.width,
                    page_height: page.height,
                },
            );
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests;
