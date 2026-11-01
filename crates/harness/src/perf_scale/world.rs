//! Builds the 100-player, 16k-tile benchmark world through public simulation APIs.
use aoe_core::{EntityId, PlayerId, TileCoord, WorldPosition};
use aoe_map::{MapPackage, MapRequest, Ratio, ResourceKind};
use aoe_simulation::{BuildingKind, GameWorld, WorldLimits};
use std::{collections::BTreeMap, error::Error, time::Instant};

pub(super) type BoxError = Box<dyn Error>;

#[derive(Clone, Debug)]
pub(super) struct Params {
    pub players: u16,
    pub units_per_player: u32,
    pub side_tiles: u32,
    pub seed: u64,
    pub fight_pairs_size: u32,
    /// Leave every unit idle (no gathering, building or fights).
    pub idle: bool,
}

/// What the benchmark knows about one player.
pub(super) struct Player {
    pub id: PlayerId,
    pub center: TileCoord,
    pub town_center: EntityId,
    /// The seat's identity unit (the village's scout), its slot's primary unit.
    pub primary: EntityId,
    pub villagers: Vec<EntityId>,
    pub military: Vec<EntityId>,
    pub nodes: Vec<(ResourceKind, u64)>,
    pub houses: u32,
    pub foundations: Vec<EntityId>,
}

pub(super) struct Built {
    pub world: GameWorld,
    pub players: Vec<Player>,
    pub timings: BTreeMap<&'static str, f64>,
    pub notes: BTreeMap<&'static str, u64>,
}

/// A France-centred request: side metres = tiles x 2 m x 30 (ratio 30:1).
pub(super) fn package(side_tiles: u32, seed: u64) -> Result<MapPackage, BoxError> {
    let request = MapRequest {
        center_latitude_e7: 466_000_000,
        center_longitude_e7: 25_000_000,
        requested_side_meters: u64::from(side_tiles) * 2 * 30,
        compression: Ratio::new(30, 1)?,
        seed,
        ..MapRequest::default()
    };
    let package = MapPackage::new(1, request, Vec::new())?;
    if package.estimate.tiles_per_side != u64::from(side_tiles) {
        return Err(format!("map has {} tiles per side", package.estimate.tiles_per_side).into());
    }
    Ok(package)
}

/// Circular offsets by increasing distance, skipping the Town Center core.
fn offsets(radius: i32, hole: i32) -> Vec<(i32, i32)> {
    let mut all = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let d2 = dx * dx + dy * dy;
            if d2 <= radius * radius && dx.abs().max(dy.abs()) > hole {
                all.push((d2, dy, dx));
            }
        }
    }
    all.sort_unstable();
    all.into_iter().map(|(_, dy, dx)| (dx, dy)).collect()
}

fn at(tile: TileCoord) -> Option<WorldPosition> {
    WorldPosition::from_tile_center(tile).ok()
}

pub(super) fn build(params: &Params) -> Result<Built, BoxError> {
    let mut timings = BTreeMap::new();
    let mut notes = BTreeMap::new();
    let started = Instant::now();
    let mut world = GameWorld::from_map(package(params.side_tiles, params.seed)?)?;
    world.set_limits(WorldLimits {
        players: 4_096,
        buildings: 1 << 20,
        gatherers: 1 << 20,
        builders: 1 << 20,
        hunters: 1 << 20,
        animals: 1 << 20,
        projectiles: 1 << 20,
    });
    timings.insert("world_new_s", started.elapsed().as_secs_f64());

    let columns = (f64::from(params.players)).sqrt().ceil() as i32;
    let side = params.side_tiles as i32;
    let cell = side / columns;
    let village_started = Instant::now();
    let mut players = Vec::new();
    for index in 0..i32::from(params.players) {
        let id = PlayerId(index as u16);
        let base = TileCoord::new(
            (index % columns) * cell + cell / 2,
            (index / columns) * cell + cell / 2,
        );
        let mut placed = None;
        for step in 0..40 {
            let start = TileCoord::new(base.x + (step % 7) * 48 - 144, base.y + (step / 7) * 48);
            if let Ok(primary) = world.initialize_village(id, start) {
                placed = Some((start, primary));
                break;
            }
        }
        let Some((start, primary)) = placed else {
            *notes.entry("villages_failed").or_default() += 1;
            continue;
        };
        let town_center = world
            .buildings()
            .find(|b| b.player == id && b.kind == BuildingKind::TownCenter)
            .map(|b| b.id)
            .ok_or("town center missing")?;
        let rect = world.building(town_center).map(|b| b.footprint);
        let center = rect.map_or(start, |r| {
            TileCoord::new((r.min.x + r.max.x) / 2, (r.min.y + r.max.y) / 2)
        });
        let villagers = world
            .units()
            .filter(|u| u.player == id && u.kind == aoe_simulation::UnitKind::Villager)
            .map(|u| u.id)
            .collect();
        players.push(Player {
            id,
            center,
            town_center,
            primary,
            villagers,
            military: Vec::new(),
            nodes: Vec::new(),
            houses: 0,
            foundations: Vec::new(),
        });
    }
    timings.insert("villages_s", village_started.elapsed().as_secs_f64());

    if !params.idle {
        let rings = offsets(14, 5);
        for player in &mut players {
            let starters: Vec<EntityId> = player.villagers.iter().take(3).copied().collect();
            for (dx, dy) in rings.iter().step_by(11).take(24) {
                if player.foundations.len() >= 12 {
                    break;
                }
                let origin = TileCoord::new(player.center.x + dx, player.center.y + dy);
                if let Ok(id) =
                    world.issue_build(player.id, &starters, BuildingKind::House, origin, false)
                {
                    player.foundations.push(id);
                }
            }
            *notes.entry("foundations_placed").or_default() += player.foundations.len() as u64;
        }
    }
    let spawn_started = Instant::now();
    let ring = offsets(34, 4);
    for player in &mut players {
        let military_wanted = (params.units_per_player / 10) as usize;
        let villagers_wanted = params.units_per_player as usize - military_wanted;
        let mut cursor = 0;
        while (player.villagers.len() < villagers_wanted || player.military.len() < military_wanted)
            && cursor < ring.len()
        {
            let (dx, dy) = ring[cursor];
            cursor += 1;
            let Some(position) = at(TileCoord::new(player.center.x + dx, player.center.y + dy))
            else {
                continue;
            };
            let spawned = if player.military.len() < military_wanted && cursor % 9 == 0 {
                world
                    .spawn_militia(player.id, position)
                    .map(|id| player.military.push(id))
            } else if player.villagers.len() < villagers_wanted {
                world
                    .spawn_villager(player.id, position)
                    .map(|id| player.villagers.push(id))
            } else {
                world
                    .spawn_militia(player.id, position)
                    .map(|id| player.military.push(id))
            };
            if spawned.is_err() {
                *notes.entry("spawn_rejected").or_default() += 1;
            }
        }
    }
    timings.insert("spawn_units_s", spawn_started.elapsed().as_secs_f64());

    let node_started = Instant::now();
    for player in &mut players {
        for kind in [
            ResourceKind::Wood,
            ResourceKind::Food,
            ResourceKind::Gold,
            ResourceKind::Stone,
        ] {
            let mut seen = std::collections::BTreeSet::new();
            for (dx, dy) in [
                (0, 0),
                (12, 0),
                (-12, 0),
                (0, 12),
                (0, -12),
                (24, 0),
                (-24, 0),
                (0, 24),
                (0, -24),
                (18, 18),
                (-18, 18),
                (18, -18),
                (-18, -18),
            ] {
                let origin = TileCoord::new(player.center.x + dx, player.center.y + dy);
                if let Some(node) = world.nearest_resource(origin, kind, 10, None)
                    && seen.insert(node.id)
                {
                    player.nodes.push((kind, node.id));
                }
            }
        }
        if !player.nodes.iter().any(|(k, _)| *k == ResourceKind::Wood) {
            *notes.entry("players_without_wood").or_default() += 1;
        }
    }
    timings.insert("find_nodes_s", node_started.elapsed().as_secs_f64());

    let order_started = Instant::now();
    for player in &mut players {
        if params.idle {
            break;
        }
        let nodes = player.nodes.clone();
        let builder_count = player.foundations.len() * 3;
        for (slot, id) in player.foundations.clone().into_iter().enumerate() {
            let group: Vec<EntityId> = player
                .villagers
                .iter()
                .skip(3 + slot * 3)
                .take(3)
                .copied()
                .collect();
            if world.issue_construct(player.id, &group, id).is_ok() {
                player.houses += 1;
            }
        }
        *notes.entry("houses_under_construction").or_default() += u64::from(player.houses);
        for (index, id) in player.villagers.clone().into_iter().enumerate() {
            if (3..3 + builder_count).contains(&index) {
                continue;
            }
            let wanted = match index % 10 {
                0..=4 => ResourceKind::Wood,
                5..=7 => ResourceKind::Food,
                8 => ResourceKind::Gold,
                _ => ResourceKind::Stone,
            };
            let candidates: Vec<_> = nodes.iter().filter(|(k, _)| *k == wanted).collect();
            let Some((_, node)) = candidates.get(index / 10 % candidates.len().max(1)) else {
                continue;
            };
            match world.issue_gather(player.id, id, *node) {
                Ok(()) => *notes.entry("gather_assigned").or_default() += 1,
                Err(_) => *notes.entry("gather_rejected").or_default() += 1,
            }
        }
    }
    timings.insert("initial_orders_s", order_started.elapsed().as_secs_f64());

    // Fight clusters midway between neighbouring bases.
    let mut fights = 0u64;
    for pair in players.chunks(if params.idle { usize::MAX } else { 2 }) {
        if params.idle {
            break;
        }
        let [a, b] = pair else { continue };
        let mid = TileCoord::new((a.center.x + b.center.x) / 2, (a.center.y + b.center.y) / 2);
        let (mut left, mut right) = (Vec::new(), Vec::new());
        for slot in 0..params.fight_pairs_size as i32 {
            let row = mid.y + slot % 6;
            let (la, ra) = (
                at(TileCoord::new(mid.x - 4 - slot / 6, row)),
                at(TileCoord::new(mid.x + 4 + slot / 6, row)),
            );
            if let (Some(la), Some(ra)) = (la, ra)
                && let (Ok(l), Ok(r)) =
                    (world.spawn_militia(a.id, la), world.spawn_militia(b.id, ra))
            {
                left.push(l);
                right.push(r);
            }
        }
        for (l, r) in left.iter().zip(&right) {
            let _ = world.issue_attack(a.id, *l, *r);
            let _ = world.issue_attack(b.id, *r, *l);
            fights += 2;
        }
    }
    notes.insert("fight_units", fights);
    Ok(Built {
        world,
        players,
        timings,
        notes,
    })
}
