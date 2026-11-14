use super::{Result, march::at};
use aoe_core::{EntityId, PlayerId};
use aoe_simulation::{GameWorld, UnitKind};

pub(super) fn town_near(world: &mut GameWorld, requested: (i32, i32)) -> Result<EntityId> {
    for ring in 0_i32..=16 {
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) == ring
                    && let Ok(id) = world.place_town_center(
                        PlayerId(0),
                        aoe_core::TileCoord::new(requested.0 + x, requested.1 + y),
                    )
                {
                    return Ok(id);
                }
            }
        }
    }
    Err("no Town Center clearing within 16 tiles".into())
}

/// Source-map clearings are irregular: use a bounded, canonical local search.
/// Never delete trees or flatten the source to make the cohort fit.
pub(super) fn spawn_near(
    world: &mut GameWorld,
    kind: UnitKind,
    requested: (i32, i32),
) -> Result<EntityId> {
    for ring in 0_i32..=16 {
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) != ring {
                    continue;
                }
                let position = at(requested.0 + x, requested.1 + y)?;
                if world.units().any(|u| u.position == position) {
                    continue;
                }
                if let Ok(id) = world.spawn_kind(PlayerId(0), kind, position) {
                    return Ok(id);
                }
            }
        }
    }
    Err("no spawn clearing within 16 tiles".into())
}

/// Distribute ordinary accepted setup jobs across nearby real work sites.
/// Sending the whole cohort to one tree can exhaust its approach reservations.
pub(super) fn assign_steward_jobs(
    world: &mut GameWorld,
    ids: &[EntityId],
    start: (i32, i32),
) -> Result<()> {
    use aoe_core::TileCoord;
    use aoe_map::ResourceKind;
    use std::collections::BTreeMap;
    let low = TileCoord::new(start.0 - 32, start.1 - 32);
    let high = TileCoord::new(start.0 + 32, start.1 + 32);
    let aoe_simulation::Terrain::Map { generator, overlay } = world.terrain() else {
        return Err("Steward setup requires map resources".into());
    };
    let mut sites = BTreeMap::new();
    for node in overlay.starting_nodes_in(low, high) {
        if node.kind == ResourceKind::Wood && overlay.resource_available(node) {
            sites.insert(node.id, node);
        }
    }
    for y in low.y..=high.y {
        for x in low.x..=high.x {
            if let Some((_, Some(node))) =
                generator.tile_and_node_with_cancel(TileCoord::new(x, y), &|| false)?
                && node.kind == ResourceKind::Wood
                && overlay.resource_available(node)
            {
                sites.insert(node.id, node);
            }
        }
    }
    let mut sites: Vec<_> = sites.into_values().collect();
    sites.sort_unstable_by_key(|node| {
        (
            (node.tile.x - start.0).pow(2) + (node.tile.y - start.1).pow(2),
            node.id,
        )
    });
    if sites.is_empty() {
        return Err("no setup wood for Steward adoption".into());
    }
    for (n, &id) in ids.iter().enumerate() {
        if n > 0 && n % 4 == 0 {
            world.advance();
        }
        let node = sites[n % sites.len()];
        world
            .issue_gather(PlayerId(0), id, node.id)
            .map_err(|error| format!("Steward setup gather {n} at node {}: {error}", node.id))?;
    }
    Ok(())
}
