//! Staged scenarios: armies placed and ordered by hand on the real map, at a real choke, with
//! no village and no AI seat (`HeadlessMatch::bare_world`). The scenario names the choke, the
//! armies (seat, side, units by kind) and whether they attack-move or just move; `stage` checks
//! the choke against the map it is played on, puts the units on the open ground around each
//! anchor, in an order the seed shuffles, and sends every army across to the other side as one
//! formation move, the order a player gives. The orders it gave are the staging's order log.
mod place;
#[cfg(test)]
mod tests;

use super::choke::{Choke, Ground, MapGround, Tile, corridor};
use aoe_core::{EntityId, Formation, PlayerId, TileCoord, WorldPosition};
use aoe_simulation::{GameWorld, UnitKind};

/// The side of the choke an army starts on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Side {
    Near,
    Far,
}

/// One army: the seat that owns it, the side it starts on and its units, kind by kind.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct Army {
    pub seat: u16,
    pub from: Side,
    pub mix: &'static [(UnitKind, u32)],
}

impl Army {
    pub fn units(&self) -> usize {
        self.mix.iter().map(|(_, count)| *count as usize).sum()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Staging {
    /// The map the choke was surveyed on, by its content hash (64 hex digits): a staged scenario
    /// is a scenario of that map and of no other.
    pub map: &'static str,
    pub choke: &'static Choke,
    pub armies: &'static [Army],
    /// The armies attack-move (they fight what they meet on the way) rather than move.
    pub attack: bool,
}

impl Staging {
    /// Seats in play: the highest seat plus one.
    pub fn seats(&self) -> usize {
        self.armies
            .iter()
            .map(|army| usize::from(army.seat) + 1)
            .max()
            .unwrap_or(0)
    }

    pub fn units(&self) -> usize {
        self.armies.iter().map(Army::units).sum()
    }
}

/// What staging did: one line per order, in the order they were given, as the order log of the run.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct Staged {
    pub orders: Vec<String>,
    pub units: usize,
}

/// An army stands at least this many tiles from the line across the neck, on its own side: clear
/// of the neck (which reaches half its length and a tile past the line) and of the other army.
pub(super) const MARGIN: i32 = 8;

/// How far `tile` is from the middle of the cross-section along the march, in tiles: negative on
/// the near side, positive on the far side.
fn along(choke: &Choke, (x, y): Tile) -> i32 {
    let ((ax, ay), (bx, by)) = choke.ends;
    (x - (ax + bx) / 2) * choke.flow.0 + (y - (ay + by) / 2) * choke.flow.1
}

/// Whether `tile` is on `side` of the neck, beyond the margin.
fn behind(choke: &Choke, side: Side, tile: Tile) -> bool {
    match side {
        Side::Near => along(choke, tile) <= -MARGIN,
        Side::Far => along(choke, tile) >= MARGIN,
    }
}

fn centre((x, y): Tile) -> Result<WorldPosition, String> {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).map_err(|error| error.to_string())
}

/// The record of `choke` holds on this ground: the survey's own measure of the neck at its saddle
/// gives the same cross-section, width and length, and the cross-section is walkable between the
/// two tiles that block it. A map that changed, or a record copied wrongly, stops here.
pub(super) fn verify(ground: &dyn Ground, choke: &Choke) -> Result<(), String> {
    let measured = corridor(&|tile| ground.walkable(tile), choke.saddle)
        .ok_or_else(|| format!("choke {}: no neck at {:?}", choke.name, choke.saddle))?;
    let recorded = (
        choke.ends,
        choke.width_tiles,
        choke.length_tiles,
        choke.flow,
    );
    let found = (
        measured.ends,
        measured.width,
        measured.length,
        measured.flow,
    );
    if recorded != found {
        return Err(format!(
            "choke {}: recorded as {recorded:?} but this map measures {found:?} at {:?}",
            choke.name, choke.saddle
        ));
    }
    if choke.ends.0 == choke.ends.1
        || [choke.ends.0, choke.ends.1]
            .iter()
            .any(|&t| ground.walkable(t))
    {
        return Err(format!("choke {}: its ends do not block", choke.name));
    }
    if let Some(blocked) = choke
        .across()
        .into_iter()
        .find(|&tile| !ground.walkable(tile))
    {
        return Err(format!(
            "choke {}: {blocked:?} of its cross-section blocks",
            choke.name
        ));
    }
    for (anchor, side) in [(choke.near, Side::Near), (choke.far, Side::Far)] {
        if !ground.walkable(anchor) {
            return Err(format!(
                "choke {}: the anchor {anchor:?} blocks",
                choke.name
            ));
        }
        if !behind(choke, side, anchor) {
            return Err(format!(
                "choke {}: the anchor {anchor:?} is not {MARGIN} tiles behind the line on its side",
                choke.name
            ));
        }
    }
    Ok(())
}

/// Puts the armies of `staging` on `world` and orders them across. `seed` shuffles which unit
/// takes which place; the same seed always gives the same layout.
pub(super) fn stage(world: &mut GameWorld, staging: &Staging, seed: u64) -> Result<Staged, String> {
    let choke = staging.choke;
    // For each army, its kinds in the order the seed gives, and the places to try, nearest first.
    let mut plans: Vec<(Vec<UnitKind>, Vec<Tile>)> = Vec::new();
    {
        let ground = MapGround::of(world).ok_or("a staged scenario is played on a map")?;
        verify(&ground, choke)?;
        for army in staging.armies {
            let anchor = match army.from {
                Side::Near => choke.near,
                Side::Far => choke.far,
            };
            let count = army.units();
            // More places than units: the world can refuse a place the map allows.
            let places = place::fill(
                &|tile| ground.walkable(tile) && behind(choke, army.from, tile),
                anchor,
                count * 5 / 4 + 16,
                40,
            );
            let kinds: Vec<UnitKind> = army
                .mix
                .iter()
                .flat_map(|&(kind, n)| std::iter::repeat_n(kind, n as usize))
                .collect();
            // The seed deals the kinds and the places out in another order: who stands where, and
            // so which unit has which id (the tie-break of the engine), is the layout.
            let salt = u64::from(army.seat) << 48;
            let order = place::shuffled(kinds.len(), seed ^ salt);
            let head = places.len().min(count);
            let dealt = place::shuffled(head, seed ^ salt ^ 0xD1B5_4A32);
            let mut layout: Vec<Tile> = dealt.into_iter().map(|n| places[n]).collect();
            layout.extend_from_slice(&places[head..]);
            plans.push((order.into_iter().map(|n| kinds[n]).collect(), layout));
        }
    }
    let mut orders = Vec::new();
    let mut units = 0;
    for (army, (kinds, places)) in staging.armies.iter().zip(plans) {
        let player = PlayerId(army.seat);
        let mut ids: Vec<EntityId> = Vec::with_capacity(kinds.len());
        let mut places = places.into_iter();
        for kind in kinds {
            let id = places
                .by_ref()
                .find_map(|tile| world.spawn_kind(player, kind, centre(tile).ok()?).ok())
                .ok_or_else(|| {
                    format!(
                        "choke {}: no place for {kind:?} of seat {} ({} placed of {})",
                        choke.name,
                        army.seat,
                        ids.len(),
                        army.units()
                    )
                })?;
            ids.push(id);
        }
        units += ids.len();
        let goal = match army.from {
            Side::Near => choke.far,
            Side::Far => choke.near,
        };
        world
            .issue_formation_move(
                player,
                &ids,
                Formation::Line,
                centre(goal)?,
                false,
                staging.attack,
            )
            .map_err(|error| format!("the order of seat {}: {error:?}", army.seat))?;
        orders.push(format!(
            "0 {} {} {} {} units to {goal:?}",
            army.seat,
            orders.len(),
            if staging.attack {
                "attack-move"
            } else {
                "move"
            },
            ids.len()
        ));
    }
    Ok(Staged { orders, units })
}
