//! Local source pages stay outside Git. Validate every ordered root before use.
use super::Result;
use aoe_map::*;
use std::{collections::BTreeMap, fs, path::Path, sync::Arc};

#[derive(Debug)]
pub(super) struct Pages(BTreeMap<EnvironmentPageKey, Arc<EnvironmentPage>>);
impl EnvironmentPageProvider for Pages {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> std::result::Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        self.0
            .get(&key)
            .cloned()
            .ok_or(EnvironmentPageError::Missing)
    }
}
pub(super) fn load(directory: &Path, hash: &str) -> Result<(MapPackage, Arc<Pages>)> {
    if hash.len() != 64 || !hash.bytes().all(|v| v.is_ascii_hexdigit()) {
        return Err("source hash must be 64 hexadecimal characters".into());
    }
    let package: MapPackage =
        serde_json::from_slice(&fs::read(directory.join(format!("{hash}.json")))?)?;
    package.validate()?;
    if package.content_hash_hex() != hash || package.source_locks.is_empty() {
        return Err(
            "quality source package must match its hash and carry real source locks".into(),
        );
    }
    let root = directory.join("pages").join(hash);
    let mut pages = BTreeMap::new();
    for (layer, folder) in [
        (PageLayer::Elevation, "elevation"),
        (PageLayer::Water, "water"),
        (PageLayer::Vegetation, "vegetation"),
        (PageLayer::HistoricalLandUse, "historical-land-use"),
        (PageLayer::HydrologyEvidence, "hydrology-evidence"),
        (PageLayer::ModernLandCover, "modern-land-cover"),
    ] {
        let path = root.join(folder);
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            if entry.path().extension().is_none_or(|e| e != "json") {
                continue;
            }
            let bytes = fs::read(entry.path())?;
            let page = match layer {
                PageLayer::Elevation => EnvironmentPage::Elevation(serde_json::from_slice(&bytes)?),
                PageLayer::Water => EnvironmentPage::Water(serde_json::from_slice(&bytes)?),
                PageLayer::Vegetation => {
                    EnvironmentPage::Vegetation(serde_json::from_slice(&bytes)?)
                }
                PageLayer::HistoricalLandUse => {
                    EnvironmentPage::HistoricalLandUse(serde_json::from_slice(&bytes)?)
                }
                PageLayer::HydrologyEvidence => {
                    EnvironmentPage::HydrologyEvidence(serde_json::from_slice(&bytes)?)
                }
                PageLayer::ModernLandCover => {
                    EnvironmentPage::ModernLandCover(serde_json::from_slice(&bytes)?)
                }
            };
            if pages.insert(page.key(), Arc::new(page)).is_some() {
                return Err("duplicate source page".into());
            }
        }
    }
    for (layer, field) in [
        (PageLayer::Elevation, Some(&package.environment.elevation)),
        (PageLayer::Water, package.environment.water.as_ref()),
        (
            PageLayer::Vegetation,
            package.environment.vegetation.as_ref(),
        ),
        (
            PageLayer::HistoricalLandUse,
            package.environment.historical_land_use.as_ref(),
        ),
    ] {
        let field = field.ok_or("quality source needs all environmental layers")?;
        for (level, metadata) in field.levels.iter().enumerate() {
            verify_root(&pages, layer, level as u8, metadata.ordered_page_root)?;
        }
    }
    let hydro = package
        .environment
        .hydrology_evidence
        .as_ref()
        .ok_or("quality source needs typed hydrology")?;
    verify_root(
        &pages,
        PageLayer::HydrologyEvidence,
        0,
        hydro.hydrology_page_root,
    )?;
    verify_root(
        &pages,
        PageLayer::ModernLandCover,
        0,
        hydro.modern_land_cover_page_root,
    )?;
    for page in pages.values() {
        if let EnvironmentPage::HydrologyEvidence(page) = page.as_ref() {
            hydro.validate_page(page, package.estimate.tiles_per_side)?;
        } else {
            page.validate()?;
        }
    }
    Ok((package, Arc::new(Pages(pages))))
}
fn verify_root(
    pages: &BTreeMap<EnvironmentPageKey, Arc<EnvironmentPage>>,
    layer: PageLayer,
    level: u8,
    expected: [u8; 32],
) -> Result<()> {
    let mut ordered: Vec<_> = pages
        .iter()
        .filter(|(k, _)| k.layer == layer && k.level == level)
        .collect();
    ordered.sort_by_key(|(k, _)| (k.y, k.x));
    let mut root = PageRootBuilder::new(layer, ordered.len())?;
    for (_, page) in ordered {
        root.push(page.content_hash()?)?;
    }
    if root.finish()? != expected {
        return Err("source page ordered root mismatch".into());
    }
    Ok(())
}

/// Find a crossing on the untouched geographical river, then add only its deck.
/// Unlike the UI demo this setup does not spawn an unrelated hundred-unit army.
pub(super) fn crossing(world: &mut aoe_simulation::GameWorld) -> Result<((i32, i32), (i32, i32))> {
    use aoe_core::{BridgeSpan, TileCoord};
    let side = world.config().width_tiles.min(world.config().height_tiles);
    let aoe_simulation::Terrain::Map { generator, .. } = world.terrain() else {
        return Err("quality source needs map terrain".into());
    };
    let water = |tile| -> Option<bool> {
        let (sample, node) = generator
            .tile_and_node_with_cancel(tile, &|| false)
            .ok()??;
        if sample.water == WaterKind::River {
            Some(true)
        } else if sample.water == WaterKind::None && sample.passable && node.is_none() {
            Some(false)
        } else {
            None
        }
    };
    let low = (side / 2 - 256).max(32);
    let high = (side / 2 + 256).min(side - 32);
    let mut origins: Vec<_> = (low..high)
        .step_by(4)
        .flat_map(|y| (low..high).step_by(4).map(move |x| TileCoord::new(x, y)))
        .collect();
    origins.sort_unstable_by_key(|t| ((t.x - side / 2).pow(2) + (t.y - side / 2).pow(2), t.y, t.x));
    let mut crossing = None;
    'search: for origin in origins {
        if water(origin) != Some(true) {
            continue;
        }
        for (dx, dy) in [(1, 1), (1, -1)] {
            let bank = |sign| {
                (1..=24)
                    .map(|n| TileCoord::new(origin.x + sign * dx * n, origin.y + sign * dy * n))
                    .find(|&t| water(t) == Some(false))
            };
            let (Some(from), Some(to)) = (bank(-1), bank(1)) else {
                continue;
            };
            let Some(span) = BridgeSpan::new(from, to) else {
                continue;
            };
            if !span
                .tiles()
                .into_iter()
                .all(|t| water(t) == Some(t != from && t != to))
            {
                continue;
            }
            let start = (from.x - dx * 20, from.y - dy * 20);
            let end = (to.x + dx * 24, to.y + dy * 24);
            if water(TileCoord::new(start.0, start.1)) != Some(false)
                || water(TileCoord::new(end.0, end.1)) != Some(false)
            {
                continue;
            }
            crossing = Some((span, start, end));
            break 'search;
        }
    }
    let (span, start, end) = crossing.ok_or("no source river crossing in bounded search")?;
    world.place_finished_bridge(span)?;
    Ok((start, end))
}
