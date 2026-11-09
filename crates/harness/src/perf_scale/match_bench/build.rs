//! The owner-shaped late game, scaled, in one process: a procedural France
//! map, one village per seat on a grid, `GameWorld::seed_late_game` with the
//! owner's seat shapes times `scale`, a Steward garrisoned in each main Town
//! Center and two Generals with slot 0's army. Everything walks fixed orders,
//! so the same options give the same world.
use super::super::world::{BoxError, package};
use aoe_core::{EntityId, PlayerId, TileCoord, WorldPosition};
use aoe_simulation::{BuildingKind, GameWorld, LateGameSeat, UnitKind, WorldLimits};
use std::time::Instant;

/// Map side of the owner's 100-seat match (package dc7e0d4b, 2,000 tiles).
pub(super) const OWNER_SIDE: f64 = 2_000.0;
pub(super) const OWNER_SEATS: f64 = 100.0;

#[derive(Clone, Debug)]
pub(super) struct Shape {
    pub seats: u16,
    pub scale: f64,
    /// 0 picks the owner's density: 2,000 tiles at 100 seats and scale 1,
    /// growing with the square root of seats times scale.
    pub side_tiles: u32,
    pub seed: u64,
}

impl Shape {
    pub(super) fn side(&self) -> u32 {
        if self.side_tiles > 0 {
            return self.side_tiles;
        }
        let area = f64::from(self.seats) / OWNER_SEATS * self.scale.max(0.01);
        // Even, so the procedural package's side is exact.
        ((OWNER_SIDE * area.sqrt() / 2.0).round() as u32 * 2).max(256)
    }
}

pub(super) struct Seat {
    pub id: PlayerId,
    pub center: TileCoord,
    pub town_center: Option<EntityId>,
    /// Living units the seat started with; the bots train back to it.
    pub target: u64,
}

pub(super) struct Built {
    pub world: GameWorld,
    pub seats: Vec<Seat>,
    pub generals: Vec<EntityId>,
    pub stewards: u64,
    pub setup_s: f64,
    pub map_s: f64,
    pub side: u32,
    pub villages_failed: u64,
}

fn main_town_center(world: &GameWorld, id: PlayerId) -> Option<(EntityId, TileCoord)> {
    world
        .buildings()
        .filter(|b| b.player == id && b.kind == BuildingKind::TownCenter)
        .min_by_key(|b| b.id)
        .map(|b| {
            let r = b.footprint;
            (
                b.id,
                TileCoord::new((r.min.x + r.max.x) / 2, (r.min.y + r.max.y) / 2),
            )
        })
}

pub(super) fn build(shape: &Shape) -> Result<Built, BoxError> {
    let started = Instant::now();
    let side = shape.side();
    let mut world = GameWorld::from_map(package(side, shape.seed)?)?;
    world.set_work_sleep(super::WORK_SLEEP.load(std::sync::atomic::Ordering::Relaxed));
    // The simulation's seat storage follows the match, not the wire's
    // 128-seat `MAX_ECONOMY_PLAYERS`: 1,000 seats need no other change.
    world.set_limits(WorldLimits::for_match(usize::from(shape.seats)));
    let map_s = started.elapsed().as_secs_f64();
    let columns = f64::from(shape.seats).sqrt().ceil() as i32;
    let cell = side as i32 / columns;
    let mut seats = Vec::new();
    let mut villages_failed = 0;
    let mut stewards = 0;
    for index in 0..i32::from(shape.seats) {
        let id = PlayerId(index as u16);
        let base = TileCoord::new(
            (index % columns) * cell + cell / 2,
            (index / columns) * cell + cell / 2,
        );
        // Fixed probe order around the cell centre, as perf-scale places.
        let spread = (cell / 8).max(8);
        let placed = (0..49).find_map(|step| {
            let start = TileCoord::new(
                base.x + (step % 7 - 3) * spread,
                base.y + (step / 7 - 3) * spread,
            );
            world.initialize_village(id, start).ok().map(|_| start)
        });
        let Some(start) = placed else {
            villages_failed += 1;
            continue;
        };
        let (town_center, center) = match main_town_center(&world, id) {
            Some((tc, center)) => (Some(tc), center),
            None => (None, start),
        };
        let owner = if index == 0 {
            LateGameSeat::owner_human()
        } else {
            LateGameSeat::owner_ai(index as usize)
        };
        let seat = owner.scaled(shape.scale);
        // A seat that refuses is left with its Dark Age kit, as the server does.
        let _ = world.seed_late_game(id, center, &seat);
        let steward_error = |error| format!("match-bench: seat {index} required Steward: {error}");
        let tc = town_center.ok_or_else(|| steward_error("main Town Center missing".to_owned()))?;
        let rect = world
            .building(tc)
            .ok_or("main Town Center missing")?
            .footprint;
        // Spawn adjacent, so garrisoning does not depend on pathfinding from
        // a fixed offset that may be blocked by the seeded late-game buildings.
        let tile = (rect.min.y - 1..=rect.max.y)
            .flat_map(|y| (rect.min.x - 1..=rect.max.x).map(move |x| TileCoord::new(x, y)))
            .find(|tile| !rect.contains(*tile) && world.tile_passable(*tile))
            .ok_or_else(|| steward_error("no passable Town Center perimeter tile".to_owned()))?;
        let at = WorldPosition::from_tile_center(tile)
            .map_err(|error| steward_error(format!("spawn position: {error}")))?;
        let steward = world
            .spawn_kind(id, UnitKind::Steward, at)
            .map_err(|error| steward_error(format!("spawn failed: {error}")))?;
        world
            .issue_garrison(id, steward, tc)
            .map_err(|error| steward_error(format!("garrison failed: {error}")))?;
        stewards += 1;
        seats.push(Seat {
            id,
            center,
            town_center,
            target: 0,
        });
    }
    let mut counts = vec![0u64; usize::from(shape.seats)];
    for unit in world.units() {
        if let Some(n) = counts.get_mut(usize::from(unit.player.0)) {
            *n += 1;
        }
    }
    for seat in &mut seats {
        seat.target = counts[usize::from(seat.id.0)];
    }
    let mut generals = Vec::new();
    if let Some(human) = seats.first() {
        for dx in [-70, 70] {
            if let Ok(at) =
                WorldPosition::from_tile_center(TileCoord::new(human.center.x + dx, human.center.y))
                && let Ok(general) = world.spawn_kind(human.id, UnitKind::General, at)
            {
                generals.push(general);
            }
        }
    }
    Ok(Built {
        world,
        seats,
        generals,
        stewards,
        setup_s: started.elapsed().as_secs_f64(),
        map_s,
        side,
        villages_failed,
    })
}
