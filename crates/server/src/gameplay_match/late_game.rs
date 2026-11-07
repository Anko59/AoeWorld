//! `start_units.late_game`: the owner's 100-seat late game as a seeded start
//! (`scale-match --late-game`). Shapes come from the owner's match of
//! 2026-11-06 (34 minutes, ticks 131,673-151,193): slot 0 a 1,650-pop
//! Imperial human with 25 Town Centers, 16 Castles, 220 stone wall tiles, 12
//! bridges and an 870-soldier army; the 99 AIs mostly Castle Age, about 27
//! villagers and 50 soldiers each. The bots then play from it; the harness
//! marches slot 0's army.
use aoe_core::{EntityId, PlayerId};
use aoe_simulation::{Age, BuildingKind as B, GameWorld, LateGameSeat, UnitKind as U};

/// Slot 0: the owner's own base and army.
fn human() -> LateGameSeat {
    LateGameSeat {
        age: Age::Imperial,
        villagers: 728,
        fishing_ships: 51,
        army: vec![
            (U::Champion, 314),
            (U::Arbalester, 230),
            (U::Paladin, 142),
            (U::EliteSkirmisher, 87),
            (U::Hussar, 63),
            (U::Halberdier, 20),
        ],
        buildings: vec![
            (B::TownCenter, 24),
            (B::Castle, 16),
            (B::Mill, 54),
            (B::LumberCamp, 69),
            (B::MiningCamp, 13),
            (B::Barracks, 14),
            (B::ArcheryRange, 15),
            (B::Stable, 24),
            (B::WatchTower, 19),
            (B::Blacksmith, 1),
            (B::University, 1),
            (B::Market, 1),
            (B::Farm, 381),
            (B::House, 259),
        ],
        wall_tiles: 220,
        bridges: 12,
        radius: 200,
    }
}

/// An AI seat: the owner's AIs were 69 Castle, 17 Feudal, 10 Imperial and
/// 4 Dark of 100; here every tenth seat is Imperial, every sixth Feudal.
fn ai(seat: usize) -> LateGameSeat {
    let age = if seat.is_multiple_of(10) {
        Age::Imperial
    } else if seat.is_multiple_of(6) {
        Age::Feudal
    } else {
        Age::Castle
    };
    LateGameSeat {
        age,
        villagers: 27,
        fishing_ships: 1,
        army: vec![
            (U::Pikeman, 14),
            (U::Crossbowman, 13),
            (U::Skirmisher, 11),
            (U::Spearman, 7),
            (U::Knight, 2),
            (U::Halberdier, 1),
            (U::Galley, 1),
        ],
        buildings: vec![
            (B::Mill, 5),
            (B::LumberCamp, 5),
            (B::MiningCamp, 1),
            (B::Barracks, 1),
            (B::ArcheryRange, 1),
            (B::Stable, 1),
            (B::Blacksmith, 1),
            (B::Farm, 3),
            (B::House, 19),
        ],
        wall_tiles: 0,
        bridges: 0,
        radius: 60,
    }
}

/// Seeds every seat around its scout; returns the units placed.
pub(super) fn seed(
    world: &mut GameWorld,
    scouts: &[EntityId],
    cancelled: &dyn Fn() -> bool,
) -> u64 {
    let mut units = 0;
    for (index, scout) in scouts.iter().enumerate() {
        if cancelled() {
            break;
        }
        let Some(center) = world.unit(*scout).map(|u| u.position.tile_floor()) else {
            continue;
        };
        let seat = if index == 0 { human() } else { ai(index) };
        match world.seed_late_game(PlayerId(index as u16), center, &seat) {
            Ok(seeded) => {
                units += seeded.units;
                tracing::info!(
                    seat = index,
                    units = seeded.units,
                    buildings = seeded.buildings,
                    wall_tiles = seeded.wall_tiles,
                    bridges = seeded.bridges,
                    gather_orders = seeded.gather_orders,
                    "late-game seat seeded"
                );
            }
            Err(error) => tracing::warn!(seat = index, %error, "late-game seat not seeded"),
        }
    }
    units
}

/// Units the late-game start may add per seat (the setup bound).
pub(super) fn units_per_seat() -> u64 {
    let total = |s: LateGameSeat| {
        u64::from(s.villagers + s.fishing_ships)
            + s.army.iter().map(|(_, n)| u64::from(*n)).sum::<u64>()
    };
    total(human()).max(total(ai(1)))
}
