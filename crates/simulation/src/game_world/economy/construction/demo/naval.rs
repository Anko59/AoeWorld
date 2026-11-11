//! Local naval skirmish: two seats of War Galleys already in range on open water.
use super::kingdom::{coast_water, kingdom_position, kingdom_water_package};
use crate::{BuildingKind, EconomyError, GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId, Seed, TileCoord};

impl GameWorld {
    /// The coast the naval demo fights on; the server publishes it so the
    /// client draws the sea.
    pub fn naval_battle_environment(
        seed: Seed,
    ) -> Result<
        (
            aoe_map::MapPackage,
            Vec<aoe_map::ElevationPage>,
            Vec<aoe_map::WaterPage>,
        ),
        EconomyError,
    > {
        kingdom_water_package(seed, 15_360, coast_water())
    }

    /// Three War Galleys per seat, four tiles apart on the first water east of
    /// a valid Dock shore, each ordered to attack the opposing galley in its row. Hulls carry five
    /// times the hit points so the exchange lasts long enough to film.
    pub fn naval_battle_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let (package, elevation, water) = Self::naval_battle_environment(seed)?;
        let mut w =
            GameWorld::from_prepared_map(package, elevation, water, Vec::new(), Vec::new())?;
        w.set_fog_disabled(true);
        let mut shore = None;
        'shore: for y in 20..100 {
            for x in 4..w.config.width_tiles - 40 {
                let tile = TileCoord::new(x, y);
                if w.validate_building_placement(BuildingKind::Dock, tile)
                    .is_ok()
                {
                    shore = Some(tile);
                    break 'shore;
                }
            }
        }
        let shore = shore.ok_or(EconomyError::InvalidFootprint)?;
        let mut rows = Vec::new();
        for row in 0..3 {
            let y = shore.y + 4 * row;
            let mut pair = [None; 2];
            for (seat, slot) in pair.iter_mut().enumerate() {
                let player = PlayerId(seat as _);
                for dx in 0..30 {
                    let x = shore.x + 12 + 4 * seat as i32 + dx;
                    let at = kingdom_position(TileCoord::new(x, y))?;
                    if let Ok(id) = w.spawn_kind(player, UnitKind::WarGalley, at) {
                        let hull = &mut w.units[w.lookup[&id]].state;
                        hull.hp *= 5;
                        hull.max_hp *= 5;
                        *slot = Some(id);
                        break;
                    }
                }
            }
            rows.push(pair);
        }
        let mut primary = None;
        for [a, b] in rows {
            let (a, b) = (
                a.ok_or(EconomyError::Capacity)?,
                b.ok_or(EconomyError::Capacity)?,
            );
            w.issue_attack(PlayerId(0), a, b)?;
            w.issue_attack(PlayerId(1), b, a)?;
            primary.get_or_insert(a);
        }
        Ok((w, primary.ok_or(EconomyError::Capacity)?))
    }
}
