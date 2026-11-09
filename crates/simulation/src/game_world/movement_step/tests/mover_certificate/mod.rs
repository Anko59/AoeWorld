//! Mover certificate (`speed/mover-certificate`): an ordinary proposal whose
//! padded sweep proves separation may skip the ordered re-sweep in `apply_step`.
//! The change must be result-identical, so these tests pin what the unchanged
//! engine does: whole replays of crowded scenes (`pins`) and the single ticks in
//! which an earlier mover closes a later mover's proposed step (`invalidation`).
//! `counts` states what the change must deliver, `watch` what ends a certificate
//! inside the ordered loop.
use crate::game_world::movement_step::FastStep;
use crate::{GameUnit, GameWorld, MovementOrder, UnitKind, UnitTask};
use aoe_core::{CombatStance, EntityId, PlayerId, Seed, TileCoord, WorldConfig, WorldPosition};

mod counts;
mod invalidation;
mod pin_values;
mod pins;
mod watch;

/// Ticks between two marks of a replay pin.
const MARK: usize = 50;
/// The eight ways out of a tile.
const DX: [i32; 8] = [1, 1, 0, -1, -1, -1, 0, 1];
const DY: [i32; 8] = [0, 1, 1, 1, 0, -1, -1, -1];

/// Runs before every tick of a replay, with the tick's number from 1.
type Script = fn(&mut GameWorld, usize);

/// What a replay of the unchanged engine produced (`pin_values`).
struct Pin {
    /// Every tick's changed ids and canonical hash, chained.
    digest: &'static str,
    /// Four bytes of the canonical hash every `MARK` ticks: a mismatch names
    /// its first window.
    marks: &'static str,
    /// Proposals the ordered apply loop refused: clear in the snapshot, closed
    /// by an earlier mover of the same tick. A wrong certificate takes them.
    refused: usize,
    /// Ticks that ended with bodies of two players interpenetrating.
    overlaps: usize,
}

#[derive(Default)]
struct Seen {
    digest: String,
    marks: Vec<String>,
    refused: usize,
    overlaps: usize,
    proposed: usize,
}

fn open_world() -> GameWorld {
    world_with_speed(128, 1)
}

/// Uniform open ground. A Scout of a player without an economy walks at the
/// configured speed: 128 subunits a tick by default.
fn world_with_speed(speed: i32, denominator: u64) -> GameWorld {
    let config = WorldConfig {
        move_speed_subunits_per_tick: speed,
        move_speed_subunits_per_tick_denominator: denominator,
        ..WorldConfig::new(256, 256, Seed(0)).unwrap()
    };
    GameWorld::new(config).unwrap()
}

fn centre(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

fn shifted(at: WorldPosition, dx: i32, dy: i32) -> WorldPosition {
    WorldPosition::new(at.x + dx, at.y + dy)
}

fn squared(a: WorldPosition, b: WorldPosition) -> i64 {
    let (x, y) = (i64::from(a.x - b.x), i64::from(a.y - b.y));
    x * x + y * y
}

/// A body that never starts a fight: these scenes are about traffic.
fn spawn(world: &mut GameWorld, player: u16, kind: UnitKind, at: WorldPosition) -> EntityId {
    let id = world.spawn_kind(PlayerId(player), kind, at).unwrap();
    // Villagers have no stance; every military kind accepts this one.
    let _ = world.issue_stance(PlayerId(player), &[id], CombatStance::NoAttack);
    id
}

/// One finished road tile; its owner needs no units.
fn pave(world: &mut GameWorld, x: i32, y: i32) {
    let tile = aoe_core::TileRect::from_xywh(x, y, 1, 1);
    let road = world.insert_building(PlayerId(9), crate::BuildingKind::Road, tile, true);
    road.unwrap();
}

/// One straight segment from where the unit stands, as a leg of a route is.
/// `issue_move` snaps lanes to tile centres; this keeps a sub-tile lane.
fn walk(world: &mut GameWorld, id: EntityId, to: WorldPosition) {
    let index = world.lookup[&id];
    let unit = &mut world.units[index];
    let from = unit.state.position;
    let (dx, dy) = (i64::from(to.x - from.x), i64::from(to.y - from.y));
    unit.state.moving = true;
    unit.state.task = UnitTask::Moving;
    unit.order = Some(MovementOrder {
        origin: from,
        destination: to,
        waypoint: to,
        target_tile: to.tile_floor(),
        segment_length: crate::game_path::segment_length(dx, dy),
        travelled: 0,
        speed_carry: 0,
    });
    if let Err(at) = world.active_movers.binary_search(&id) {
        world.active_movers.insert(at, id);
    }
    world.index_destination(id);
}

/// The steps the coming tick proposes, by mover. Exact on uniform ground
/// without groups: nothing searches or rejoins before the proposals.
fn proposals(world: &GameWorld) -> Vec<(EntityId, FastStep)> {
    let movers = world.active_movers.clone();
    let mut found = Vec::new();
    if movers.is_empty() {
        return found;
    }
    let start = world.planning_cursor % movers.len();
    let steps = world.propose_steps(&movers, start);
    for (turn, step) in steps.into_iter().enumerate() {
        if let Some(step) = step {
            found.push((movers[(start + turn) % movers.len()], step));
        }
    }
    found
}

fn proposal_of(world: &GameWorld, id: EntityId) -> Option<FastStep> {
    let found = proposals(world).into_iter().find(|(mover, _)| *mover == id);
    found.map(|(_, step)| step)
}

/// True when the sequential path refused the unit's step in tick `now`: it
/// recorded an obstruction, which an accepted proposal never does.
fn blocked_at(world: &GameWorld, id: EntityId, now: u64) -> bool {
    let obstruction = world.units[world.lookup[&id]].obstruction;
    obstruction.is_some_and(|o| o.last_tick == now)
}

/// Pairs of live bodies of different players closer than their radii sum.
/// Every sweep keeps that distance between players (`HARD_OTHER`).
fn foreign_overlaps(world: &GameWorld) -> usize {
    let mut bodies: Vec<GameUnit> = world.units().filter(|unit| unit.hp > 0).collect();
    bodies.sort_unstable_by_key(|unit| (unit.position.x, unit.id));
    let mut overlaps = 0;
    for (n, a) in bodies.iter().enumerate() {
        for b in &bodies[n + 1..] {
            if b.position.x - a.position.x >= 1024 {
                break;
            }
            let reach = i64::from(a.kind.collision_radius() + b.kind.collision_radius());
            if a.player != b.player && squared(a.position, b.position) < reach * reach {
                overlaps += 1;
            }
        }
    }
    overlaps
}

fn replay(mut world: GameWorld, ticks: usize, count: bool, script: Script) -> Seen {
    let mut chain = blake3::Hasher::new();
    let mut seen = Seen::default();
    for tick in 1..=ticks {
        script(&mut world, tick);
        let now = world.tick.0;
        let mut proposed = Vec::new();
        if count {
            proposed = proposals(&world);
        }
        for unit in world.advance() {
            chain.update(&unit.id.0.to_le_bytes());
        }
        let hash = world.canonical_hash();
        chain.update(&hash);
        if tick % MARK == 0 {
            let mark = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]);
            seen.marks.push(format!("{mark:08x}"));
        }
        seen.proposed += proposed.len();
        for (id, _) in &proposed {
            seen.refused += usize::from(blocked_at(&world, *id, now));
        }
        seen.overlaps += usize::from(foreign_overlaps(&world) > 0);
    }
    seen.digest = chain.finalize().to_hex().to_string();
    seen
}

/// Replays the scene twice (plain, then reading every tick's proposals
/// first) and compares it with its pin.
fn assert_pin(name: &str, build: fn() -> GameWorld, ticks: usize, script: Script, pin: &Pin) {
    let plain = replay(build(), ticks, false, script);
    let seen = replay(build(), ticks, true, script);
    assert_eq!(plain.digest, seen.digest, "{name}: proposals are pure");
    let (digest, marks) = (seen.digest, seen.marks.join(" "));
    let (refused, overlaps, proposed) = (seen.refused, seen.overlaps, seen.proposed);
    let same = digest == pin.digest
        && marks == pin.marks
        && refused == pin.refused
        && overlaps == pin.overlaps;
    let window = marks
        .split(' ')
        .zip(pin.marks.split(' '))
        .position(|(now, pinned)| now != pinned);
    assert!(
        same,
        "{name}: the replay changed, first in {MARK}-tick window {window:?}. The mover \
         certificate must be result-identical; regenerate `pin_values` only for an \
         intended behaviour change ({proposed} proposals read):\n\
         PIN {name} digest: \"{digest}\", marks: \"{marks}\", refused: {refused}, \
         overlaps: {overlaps},"
    );
}
