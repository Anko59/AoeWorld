//! A synthetic coast for the browser scenario and local showcase, never geodata.
use super::*;
use aoe_core::{PlayerId, TileCoord};
use aoe_map::{ObjectKind, ResourceNode};
use aoe_simulation::{BuildingKind, GameWorld, StartSearchResult};

#[cfg(test)]
#[path = "tests/fishing.rs"]
mod tests;

pub(crate) fn prepare(root: &Path, directory: &Path) -> Result<String> {
    let mut base = fields()?;
    let water = [
        WaterPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            ocean_coverage_percent: vec![0, 100, 0, 100],
            inland_coverage_percent: vec![0; 4],
        },
        WaterPage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            ocean_coverage_percent: vec![50],
            inland_coverage_percent: vec![0],
        },
    ];
    let mut levels = Vec::new();
    for page in &water {
        levels.push(PyramidLevel {
            samples_per_axis: u16::from(page.width),
            ordered_page_root: ordered_water_page_root(std::slice::from_ref(page))?,
        });
    }
    base.package.environment.water = Some(FieldPyramid { levels });
    base.package.request.requested_side_meters = 7680;
    let package = MapPackage::with_prepared_environment(
        1,
        base.package.request,
        Vec::new(),
        ProjectionMetadata {
            horizontal_crs: "synthetic-fishing-coast".into(),
            ..base.package.projection
        },
        base.package.provenance,
        base.package.environment,
    )?;
    let hash = package.content_hash_hex();
    let generator = package.generator_with_environment(
        vec![base.elevation.clone()],
        water.to_vec(),
        vec![base.vegetation.clone()],
        Vec::new(),
    )?;
    let mut world = GameWorld::from_prepared_map(
        package.clone(),
        vec![base.elevation.clone()],
        water.to_vec(),
        vec![base.vegetation.clone()],
        Vec::new(),
    )?;
    let config = world.config();
    let StartSearchResult::Found(start) =
        world.terrain().search_start_checked(config, 64, || false)?
    else {
        return Err("no coastal village start".into());
    };
    world.initialize_village(PlayerId(0), start)?;
    let dock = (-12_i32..=12)
        .flat_map(|dy| (80..112).map(move |x| TileCoord::new(x, start.y + dy)))
        .find(|t| {
            world
                .validate_building_placement(BuildingKind::Dock, *t)
                .is_ok()
        })
        .ok_or_else(|| {
            format!(
                "no dock site (start {start:?}, width {}, row {:?})",
                config.width_tiles,
                (0..128)
                    .step_by(8)
                    .filter_map(|x| generator
                        .tile_at(TileCoord::new(x, 64))
                        .map(|t| (x, t.water, t.passable)))
                    .collect::<Vec<_>>()
            )
        })?;
    let mut nodes = Vec::new();
    for cy in 0..4 {
        for cx in 0..4 {
            nodes.extend(generator.chunk(cx, cy)?.resources);
        }
    }
    let nearest = |kind| -> Result<ResourceNode> {
        nodes
            .iter()
            .filter(|n| n.object == kind)
            .filter(|n| kind != ObjectKind::ShoreFish || n.tile.y.abs_diff(dock.y + 1) > 4)
            .filter(|n| {
                kind != ObjectKind::ShoreFish
                    || [(0, -1), (-1, 0), (1, 0), (0, 1)].iter().any(|(dx, dy)| {
                        let tile = TileCoord::new(n.tile.x + dx, n.tile.y + dy);
                        !(tile.x >= dock.x
                            && tile.x < dock.x + 3
                            && tile.y >= dock.y
                            && tile.y < dock.y + 3)
                            && world.terrain().passable(tile, config)
                    })
            })
            .min_by_key(|n| n.tile.x.abs_diff(dock.x).pow(2) + n.tile.y.abs_diff(dock.y).pow(2))
            .copied()
            .ok_or_else(|| "missing fish in coast".into())
    };
    let shore = nearest(ObjectKind::ShoreFish)?;
    let deep = nearest(ObjectKind::DeepFish)?;
    let waiting = |dy| -> Result<TileCoord> {
        (2..=6)
            .map(|dx| TileCoord::new(dock.x - dx, dock.y + dy))
            .find(|tile| world.terrain().passable(*tile, config))
            .ok_or_else(|| "missing coastal waiting tile".into())
    };
    let builder_wait = waiting(1)?;
    let fisher_wait = waiting(3)?;
    let villagers = world
        .units()
        .filter(|u| u.kind == aoe_simulation::UnitKind::Villager)
        .map(|u| (u.id, u.position.tile_floor()))
        .collect::<Vec<_>>();
    let walkers = [
        (villagers[1].0, builder_wait),
        (villagers[2].0, fisher_wait),
    ];
    for (id, tile) in walkers {
        world.issue_move(id, aoe_core::WorldPosition::from_tile_center(tile)?)?;
    }
    let mut walk_ticks = 0;
    while walk_ticks < 6000
        && walkers.iter().any(|(id, tile)| {
            world
                .unit(*id)
                .is_none_or(|u| u.position.tile_floor() != *tile)
        })
    {
        world.advance();
        walk_ticks += 1;
    }
    if walkers.iter().any(|(id, tile)| {
        world
            .unit(*id)
            .is_none_or(|u| u.position.tile_floor() != *tile)
    }) {
        return Err(format!(
            "coast not reachable: {:?}",
            walkers.map(|(id, _)| world.unit(id))
        )
        .into());
    }
    let ship_spawn = (dock.y - 1..=dock.y + 3)
        .flat_map(|y| (dock.x - 1..=dock.x + 3).map(move |x| TileCoord::new(x, y)))
        .find(|tile| {
            !(tile.x >= dock.x && tile.x < dock.x + 3 && tile.y >= dock.y && tile.y < dock.y + 3)
                && generator.tile_at(*tile).is_some_and(|t| {
                    matches!(
                        t.water,
                        aoe_map::WaterKind::Lake
                            | aoe_map::WaterKind::Ocean
                            | aoe_map::WaterKind::River
                    ) && t.material != aoe_map::GroundMaterial::Ford
                })
                && generator.object_at(*tile).is_none()
        })
        .ok_or("missing dock water perimeter")?;
    let pages = directory.join("pages").join(&hash);
    for (layer, name, bytes) in [
        ("elevation", "0-0-0", serde_json::to_vec(&base.elevation)?),
        ("vegetation", "0-0-0", serde_json::to_vec(&base.vegetation)?),
        ("water", "0-0-0", serde_json::to_vec(&water[0])?),
        ("water", "1-0-0", serde_json::to_vec(&water[1])?),
    ] {
        let target = pages.join(layer);
        fs::create_dir_all(&target)?;
        publish(&target.join(format!("{name}.json")), &bytes)?;
    }
    publish(
        &directory.join(format!("{hash}.json")),
        &serde_json::to_vec(&package)?,
    )?;
    let cache = root.join(".cache/fishing");
    fs::create_dir_all(&cache)?;
    fs::write(
        cache.join("scene.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "hash": hash, "dock": dock, "shore": shore.tile, "deep": deep.tile,
            "start": start, "ship_spawn": ship_spawn,
            "builder_wait": builder_wait, "fisher_wait": fisher_wait,
            "walk_ticks": walk_ticks,
            "villagers": villagers.iter()
                .map(|(id, tile)| serde_json::json!({"id": id.0, "tile": tile}))
                .collect::<Vec<_>>(),
        }))?,
    )?;
    Ok(hash)
}
