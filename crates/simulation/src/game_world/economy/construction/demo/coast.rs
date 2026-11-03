//! Local accelerated coast, using real construction, training and fish orders.
use super::kingdom::{clear_kingdom_plot, coastal_world, kingdom_building, kingdom_position};
use crate::{BuildingKind, EconomyError, GameWorld, Stockpile, Tech, UnitKind, VassalSettings};
use aoe_core::{CombatStance, EntityId, PlayerId, Seed, TileCoord, TileRect};
use aoe_map::{ObjectKind, ResourceNode};

impl GameWorld {
    /// One-Hz game rules under the ordinary server ticker make coastal growth
    /// observable in a short local recording; no build or training time changes.
    pub fn vassal_fishing_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let mut w = coastal_world(seed)?;
        w.config.tick_hz = 1;
        w.set_fog_disabled(true);
        let mut shore = None;
        'shore: for y in 20..100 {
            for x in 4..w.config.width_tiles - 4 {
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
        clear_kingdom_plot(
            &mut w,
            TileRect::from_xywh(shore.x - 30, shore.y - 4, 26, 34),
        )?;
        let castle_at = TileCoord::new(shore.x - 15, shore.y + 10);
        let castle = kingdom_building(&mut w, BuildingKind::Castle, castle_at)?;
        for i in 0..6 {
            kingdom_building(
                &mut w,
                BuildingKind::House,
                TileCoord::new(shore.x - 29 + 3 * i, shore.y + 2),
            )?;
        }
        w.complete_research(PlayerId(0), Tech::FeudalAge, &mut Vec::new());
        w.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
        let mut workers = 0;
        'workers: for y in castle_at.y - 1..castle_at.y + 6 {
            for x in castle_at.x - 2..castle_at.x + 6 {
                let tile = TileCoord::new(x, y);
                if w.tile_free(tile) {
                    w.spawn_villager(PlayerId(0), kingdom_position(tile)?)?;
                    workers += 1;
                    if workers == 8 {
                        break 'workers;
                    }
                }
            }
        }
        if workers != 8 {
            return Err(EconomyError::Capacity);
        }
        let exit = TileCoord::new(castle_at.x - 1, castle_at.y + 2);
        let king = w.spawn_kind(PlayerId(0), UnitKind::Vassal, kingdom_position(exit)?)?;
        w.issue_vassal_settings(
            PlayerId(0),
            king,
            VassalSettings {
                training: true,
                ..Default::default()
            },
        )?;
        w.issue_garrison(PlayerId(0), king, castle)?;
        let primary = w.spawn_kind(
            PlayerId(0),
            UnitKind::Scout,
            kingdom_position(TileCoord::new(shore.x - 7, shore.y + 9))?,
        )?;
        w.issue_stance(PlayerId(0), &[primary], CombatStance::StandGround)?;
        let mut nodes: Vec<ResourceNode> = Vec::new();
        if let crate::Terrain::Map { generator, .. } = &w.terrain {
            for y in 0..5 {
                for x in 0..8 {
                    nodes.extend(
                        generator
                            .chunk(x, y)
                            .map_err(|_| EconomyError::InvalidResource)?
                            .resources,
                    );
                }
            }
        }
        nodes.sort_unstable_by_key(|node| {
            (
                node.tile.x.abs_diff(shore.x).pow(2) + node.tile.y.abs_diff(shore.y).pow(2),
                node.id,
            )
        });
        let fish = nodes
            .into_iter()
            .find(|n| n.object == ObjectKind::DeepFish)
            .ok_or(EconomyError::InvalidResource)?;
        let mut boat = None;
        for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            let at = kingdom_position(TileCoord::new(fish.tile.x + dx, fish.tile.y + dy))?;
            if let Ok(id) = w.spawn_kind(PlayerId(0), UnitKind::FishingShip, at) {
                boat = Some(id);
                break;
            }
        }
        w.issue_gather(PlayerId(0), boat.ok_or(EconomyError::Capacity)?, fish.id)?;
        w.economy
            .players
            .get_mut(&PlayerId(0))
            .ok_or(EconomyError::NotOwner)?
            .stockpile = Stockpile {
            food: 0,
            wood: 10_000,
            gold: 10_000,
            stone: 10_000,
        };
        Ok((w, primary))
    }
}
