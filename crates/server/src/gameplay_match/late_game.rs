//! `start_units.late_game`: the owner's 100-seat late game as a seeded start
//! (`scale-match --late-game`). Shapes come from the owner's match of
//! 2026-11-06 (34 minutes, ticks 131,673-151,193): slot 0 a 1,650-pop
//! Imperial human with 25 Town Centers, 16 Castles, 220 stone wall tiles, 12
//! bridges and an 870-soldier army; the 99 AIs mostly Castle Age, about 27
//! villagers and 50 soldiers each. The bots then play from it; the harness
//! marches slot 0's army.
use aoe_core::{EntityId, PlayerId};
use aoe_simulation::{GameWorld, LateGameSeat};

/// Slot 0: the owner's own base and army.
fn human() -> LateGameSeat {
    LateGameSeat::owner_human()
}

/// An AI seat (`LateGameSeat::owner_ai`).
fn ai(seat: usize) -> LateGameSeat {
    LateGameSeat::owner_ai(seat)
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
    human().unit_target().max(ai(1).unit_target())
}
