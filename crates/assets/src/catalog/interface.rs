//! Original AoK interface art. DAT icon IDs are sheet frame indices, never SLP IDs.
use crate::{Error, dat::DatFile, invalid, pack::Manifest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Increment when the exported interface selection or crops change.
pub const VERSION: u8 = 5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconSource {
    pub slp: u32,
    pub frame: u32,
}

/// Crop within an existing pack colour page; no public copy of original pixels.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Art {
    pub page: String,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub page_width: u16,
    pub page_height: u16,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Interface {
    pub units: BTreeMap<i32, Art>,
    pub technologies: BTreeMap<i32, Art>,
    pub commands: BTreeMap<String, Art>,
    pub resources: BTreeMap<String, Art>,
    pub panels: BTreeMap<String, Art>,
}

pub fn unit_icon(dat: &DatFile, id: i32) -> Option<IconSource> {
    let unit = dat.unit(id)?;
    Some(IconSource {
        slp: if unit.kind == 80 { 50706 } else { 50730 },
        frame: u32::try_from(unit.icon_id).ok()?,
    })
}

// Cross-checked against freeaoe src/ui/ActionPanel.h's native command enum.
// AoK has no explicit Move button: reuse its pointer cursor for our Move control.
// Garrison is also a cursor, not the command sheet's Close Gate (frame 47).
const COMMANDS: &[(&str, u32, u32)] = &[
    ("move", 51000, 0),
    ("stop", 50721, 3),
    ("build", 50721, 30),
    ("military", 50721, 31),
    ("garrison", 51000, 13),
    ("ungarrison", 50721, 2),
    ("attack", 51000, 4),
    ("build-cursor", 51000, 7),
    ("repair", 50721, 28),
    ("cancel", 50721, 0),
];

pub fn command_icon(name: &str) -> Option<IconSource> {
    COMMANDS
        .iter()
        .find(|(key, _, _)| *key == name)
        .map(|&(_, slp, frame)| IconSource { slp, frame })
}

fn art(manifest: &Manifest, slp: u32, frame: u32) -> Result<Option<Art>, Error> {
    let source = format!("interfac.drs:[32, 112, 108, 115]:{slp}");
    let mut records = manifest
        .frames
        .iter()
        .filter(|f| f.source == source)
        .peekable();
    if records.peek().is_none() {
        return Ok(None);
    }
    let record = records
        .find(|f| f.frame == frame)
        .ok_or_else(|| invalid("interface", 0, format!("{slp} missing icon frame {frame}")))?;
    let page = manifest
        .pages
        .get(usize::from(record.page))
        .ok_or_else(|| invalid("interface", 0, "page missing"))?;
    Ok(Some(Art {
        page: page.color.clone(),
        x: record.x,
        y: record.y,
        width: record.width,
        height: record.height,
        page_width: page.width,
        page_height: page.height,
    }))
}

fn crop(mut art: Art, x: u16, y: u16, width: u16, height: u16) -> Result<Art, Error> {
    if u32::from(x) + u32::from(width) > u32::from(art.width)
        || u32::from(y) + u32::from(height) > u32::from(art.height)
    {
        return Err(invalid("interface", 0, "panel crop outside source"));
    }
    art.x += x;
    art.y += y;
    art.width = width;
    art.height = height;
    Ok(art)
}

/// Only roles used by the current game enter the DOM catalog. Absent whole
/// sheets are optional; a present sheet missing a referenced frame is an error.
pub fn build(dat: &DatFile, manifest: &Manifest) -> Result<Interface, Error> {
    let mut catalog = Interface::default();
    for id in super::GAMEPLAY_BUILDING_IDS.into_iter().chain([
        83, 293, 448, 74, 75, 4, 7, 93, 13, 594, 65, 48, 126, 59, 66, 102, 349, 53,
    ]) {
        if let Some(source) = unit_icon(dat, id)
            && let Some(art) = art(manifest, source.slp, source.frame)?
        {
            catalog.units.insert(id, art);
        }
    }
    for id in [22, 8, 101, 202, 14, 55, 278, 213, 222, 67, 74, 81, 199, 211] {
        if let Some(tech) = dat.technologies.iter().find(|t| t.id == id)
            && let Ok(frame) = u32::try_from(tech.icon_id)
            && let Some(art) = art(manifest, 50729, frame)?
        {
            catalog.technologies.insert(id, art);
        }
    }
    for &(name, slp, frame) in COMMANDS {
        if let Some(art) = art(manifest, slp, frame)? {
            catalog.commands.insert(name.into(), art);
        }
    }
    for (name, frame) in [("wood", 0), ("stone", 1), ("food", 2), ("gold", 3)] {
        if let Some(art) = art(manifest, 50732, frame)? {
            catalog.resources.insert(name.into(), art);
        }
    }
    // The trial ships Western and Celtic 1024×768 frames. Their top bar and
    // three bottom compartments are retained separately to fit modern widths.
    for (style, slp) in [("western", 51121), ("celtic", 51133)] {
        if let Some(frame) = art(manifest, slp, 0)? {
            for (role, x, y, w, h) in [
                ("commands", 0, 596, 276, 172),
                ("selection", 276, 596, 404, 172),
                ("minimap", 680, 596, 344, 172),
                ("top", 500, 0, 524, 24),
            ] {
                catalog
                    .panels
                    .insert(format!("{style}-{role}"), crop(frame.clone(), x, y, w, h)?);
            }
            if style == "western" {
                catalog
                    .resources
                    .insert("population".into(), crop(frame, 314, 2, 29, 19)?);
            }
        }
    }
    if let Some(art) = art(manifest, 53011, 0)? {
        catalog.panels.insert("button".into(), art);
    }
    if let Some(art) = art(manifest, 50751, 36)? {
        catalog.panels.insert("portrait".into(), art);
    }
    Ok(catalog)
}

#[cfg(test)]
#[path = "tests/interface.rs"]
mod tests;
