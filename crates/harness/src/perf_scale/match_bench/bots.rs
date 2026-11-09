//! Minimal closed-loop Rust bots: they read the world after each tick and
//! issue ordinary player orders, so behaviour changes feed back into the load
//! (an open-loop command log would diverge). Every decision walks seats, units
//! and buildings in id order, so a run is a function of its options.
//!
//! Per seat, every `THINK` ticks: idle villagers go back to the nearest wood,
//! gold, stone or food; every `TRAIN` ticks a seat under its starting unit
//! count queues a villager and a soldier. AI seats send their idle land army
//! as one Line attack-move at a neighbouring seat every `AI_WAVE` ticks
//! (staggered by seat). Slot 0 plays the owner's scripted human of
//! `scale-match --late-game`: every 45 s its army splits into four Line
//! attack-moves 300 to 700 tiles away, the direction turning each round, and
//! its two Generals march the same way.
use super::build::Seat;
use aoe_core::{EntityId, Formation, PlayerId, TileCoord, WorldPosition};
use aoe_map::ResourceKind;
use aoe_simulation::{BuildingKind, GameWorld, UnitKind, UnitTask};
use serde::Serialize;

const THINK: u64 = 20;
const TRAIN: u64 = 200;
const AI_WAVE: u64 = 1_200;
const HUMAN_WAVE: u64 = 900;
/// Idle villagers one seat re-tasks per think.
const RETASK: usize = 64;
const GATHER_RADIUS: i32 = 48;
const LEAST_WAVE: usize = 10;

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub(super) struct BotCounts {
    pub gather_ok: u64,
    pub gather_refused: u64,
    pub gather_no_node: u64,
    pub train_ok: u64,
    pub train_refused: u64,
    pub waves_ok: u64,
    pub waves_refused: u64,
    pub wave_units: u64,
    pub general_orders: u64,
}

fn land_soldier(kind: UnitKind) -> bool {
    !kind.is_naval()
        && !matches!(
            kind,
            UnitKind::Villager
                | UnitKind::Steward
                | UnitKind::Vassal
                | UnitKind::General
                | UnitKind::Monk
        )
}

#[derive(Default)]
struct SeatView {
    idle_villagers: Vec<(EntityId, TileCoord)>,
    soldiers: Vec<EntityId>,
    idle_soldiers: Vec<EntityId>,
    living: u64,
}

fn clamp(world: &GameWorld, tile: TileCoord) -> TileCoord {
    let config = world.config();
    TileCoord::new(
        tile.x.clamp(8, config.width_tiles - 9),
        tile.y.clamp(8, config.height_tiles - 9),
    )
}

/// One step of every bot; called after each tick with the tick just done.
pub(super) fn step(
    world: &mut GameWorld,
    seats: &[Seat],
    generals: &[EntityId],
    counts: &mut BotCounts,
) {
    let tick = world.tick().0;
    if !tick.is_multiple_of(THINK) {
        return;
    }
    let mut views: Vec<SeatView> = (0..seats.len()).map(|_| SeatView::default()).collect();
    let mut slot = Vec::new();
    for (index, seat) in seats.iter().enumerate() {
        let id = usize::from(seat.id.0);
        if slot.len() <= id {
            slot.resize(id + 1, None);
        }
        slot[id] = Some(index);
    }
    let index_of = |player: PlayerId| slot.get(usize::from(player.0)).copied().flatten();
    let mut units: Vec<_> = world.units().filter(|u| u.hp > 0).collect();
    units.sort_unstable_by_key(|u| u.id);
    for unit in &units {
        let Some(view) = index_of(unit.player).and_then(|i| views.get_mut(i)) else {
            continue;
        };
        view.living += 1;
        if unit.kind == UnitKind::Villager && unit.task == UnitTask::Idle && !unit.moving {
            view.idle_villagers
                .push((unit.id, unit.position.tile_floor()));
        } else if land_soldier(unit.kind) {
            view.soldiers.push(unit.id);
            if unit.task == UnitTask::Idle && !unit.moving {
                view.idle_soldiers.push(unit.id);
            }
        }
    }
    let columns = (seats.len() as f64).sqrt().ceil() as usize;
    for (index, (seat, view)) in seats.iter().zip(&views).enumerate() {
        gather(world, seat.id, &view.idle_villagers, tick, counts);
        if (tick / THINK + index as u64).is_multiple_of(TRAIN / THINK) && view.living < seat.target
        {
            train(world, seat, counts);
        }
        if index == 0 {
            if tick.is_multiple_of(HUMAN_WAVE) {
                human_wave(world, seat, &view.soldiers, generals, tick, counts);
            }
            continue;
        }
        let offset = (index as u64 * 7 * THINK) % AI_WAVE;
        if (tick + offset).is_multiple_of(AI_WAVE) && view.idle_soldiers.len() >= LEAST_WAVE {
            let round = (tick + offset) / AI_WAVE;
            let neighbours = [
                index.checked_sub(1),
                Some(index + 1),
                index.checked_sub(columns),
                Some(index + columns),
            ];
            let target = neighbours[(round as usize + index) % 4]
                .filter(|&n| n < seats.len() && n != index)
                .or(Some((index + 1) % seats.len()));
            let Some(target) = target.and_then(|t| seats.get(t)) else {
                continue;
            };
            wave(world, seat.id, &view.idle_soldiers, target.center, counts);
        }
    }
}

fn gather(
    world: &mut GameWorld,
    player: PlayerId,
    idle: &[(EntityId, TileCoord)],
    tick: u64,
    counts: &mut BotCounts,
) {
    const KINDS: [ResourceKind; 4] = [
        ResourceKind::Wood,
        ResourceKind::Gold,
        ResourceKind::Stone,
        ResourceKind::Food,
    ];
    for (n, &(id, tile)) in idle.iter().take(RETASK).enumerate() {
        let first = (n + (tick / THINK) as usize) % KINDS.len();
        let node = (0..KINDS.len()).find_map(|k| {
            world.nearest_resource(tile, KINDS[(first + k) % KINDS.len()], GATHER_RADIUS, None)
        });
        let Some(node) = node else {
            counts.gather_no_node += 1;
            continue;
        };
        match world.issue_gather(player, id, node.id) {
            Ok(()) => counts.gather_ok += 1,
            Err(_) => counts.gather_refused += 1,
        }
    }
}

fn train(world: &mut GameWorld, seat: &Seat, counts: &mut BotCounts) {
    let mut orders: Vec<(EntityId, UnitKind)> = Vec::new();
    if let Some(tc) = seat.town_center {
        orders.push((tc, UnitKind::Villager));
    }
    let mut producers: Vec<_> = world
        .buildings()
        .filter(|b| b.player == seat.id && b.is_complete())
        .filter_map(|b| {
            let kind = match b.kind {
                BuildingKind::Barracks => UnitKind::Pikeman,
                BuildingKind::ArcheryRange => UnitKind::Crossbowman,
                BuildingKind::Stable => UnitKind::Knight,
                _ => return None,
            };
            Some((b.id, kind))
        })
        .collect();
    producers.sort_unstable_by_key(|(id, _)| *id);
    orders.extend(producers.into_iter().take(3));
    for (building, kind) in orders {
        match world.issue_train(seat.id, building, kind) {
            Ok(()) => counts.train_ok += 1,
            Err(_) => counts.train_refused += 1,
        }
    }
}

fn wave(
    world: &mut GameWorld,
    player: PlayerId,
    ids: &[EntityId],
    to: TileCoord,
    counts: &mut BotCounts,
) {
    let to = clamp(world, to);
    let Ok(at) = WorldPosition::from_tile_center(to) else {
        return;
    };
    match world.issue_formation_move(player, ids, Formation::Line, at, false, true) {
        Ok(()) => {
            counts.waves_ok += 1;
            counts.wave_units += ids.len() as u64;
        }
        Err(_) => counts.waves_refused += 1,
    }
}

/// Unit vectors of 16 directions in 1/1000, turning a quarter plus a
/// sixteenth each round.
fn direction(round: u64) -> (i32, i32) {
    let angle = (round * 5 % 16) as f64 * std::f64::consts::TAU / 16.0;
    (
        (angle.cos() * 1000.0).round() as i32,
        (angle.sin() * 1000.0).round() as i32,
    )
}

fn human_wave(
    world: &mut GameWorld,
    seat: &Seat,
    soldiers: &[EntityId],
    generals: &[EntityId],
    tick: u64,
    counts: &mut BotCounts,
) {
    let round = tick / HUMAN_WAVE;
    if !soldiers.is_empty() {
        let size = soldiers.len().div_ceil(4);
        for (group, ids) in soldiers.chunks(size).enumerate() {
            let (dx, dy) = direction(round + group as u64 * 4);
            let reach = 300 + ((round * 3 + group as u64) % 5) as i32 * 100;
            let to = TileCoord::new(
                seat.center.x + dx * reach / 1000,
                seat.center.y + dy * reach / 1000,
            );
            wave(world, seat.id, ids, to, counts);
        }
    }
    let (dx, dy) = direction(round);
    let to = clamp(
        world,
        TileCoord::new(
            seat.center.x + dx * 400 / 1000,
            seat.center.y + dy * 400 / 1000,
        ),
    );
    if let Ok(at) = WorldPosition::from_tile_center(to) {
        let alive: Vec<_> = generals
            .iter()
            .copied()
            .filter(|&g| world.unit_exists(g))
            .collect();
        if !alive.is_empty() {
            world.general_player_order(seat.id, &alive, Some(at));
            counts.general_orders += 1;
        }
    }
}
