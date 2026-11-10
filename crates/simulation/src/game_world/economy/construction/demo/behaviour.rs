//! Paused, reproducible playtest encounters; commands and clocks stay ordinary.
use super::kingdom::{clear_kingdom_plot, kingdom_position, kingdom_water_package};
use crate::{AnimalKind, BuildingKind, EconomyError, GameWorld, Stockpile, Tech, UnitKind};
use aoe_core::{EntityId, PlayerId, Seed, TileCoord, TileRect};
use aoe_map::{ResourceKind, ResourceNode};

#[derive(Clone, Copy, Debug)]
pub enum BehaviourScene {
    TownCenter,
    Wolf,
    Deer,
    Hunt,
    Camps,
    Bridge,
}

impl GameWorld {
    pub fn behaviour_demo(
        seed: Seed,
        scene: BehaviourScene,
    ) -> Result<(Self, EntityId), EconomyError> {
        let (package, elevation, water) = Self::behaviour_environment(seed, scene)?;
        let mut w = Self::from_prepared_map(package, elevation, water, Vec::new(), Vec::new())?;
        w.set_fog_disabled(true);
        if matches!(scene, BehaviourScene::Hunt) {
            w.config.tick_hz = 5;
        }
        clear_kingdom_plot(&mut w, TileRect::from_xywh(80, 80, 90, 90))?;
        let home = w.place_town_center(PlayerId(0), TileCoord::new(100, 100))?;
        w.complete_research(PlayerId(0), Tech::FeudalAge, &mut Vec::new());
        w.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
        w.economy
            .players
            .get_mut(&PlayerId(0))
            .ok_or(EconomyError::UnknownEntity)?
            .stockpile = Stockpile {
            food: 10_000,
            wood: 10_000,
            gold: 10_000,
            stone: 10_000,
        };
        let start = if matches!(scene, BehaviourScene::Wolf) {
            TileCoord::new(134, 128)
        } else if matches!(scene, BehaviourScene::Bridge) {
            (80..128)
                .rev()
                .map(|x| TileCoord::new(x, 129))
                .find(|tile| w.tile_free(*tile))
                .ok_or(EconomyError::InvalidFootprint)?
        } else {
            TileCoord::new(127, 129)
        };
        let worker = w.spawn_villager(PlayerId(0), kingdom_position(start)?)?;
        match scene {
            BehaviourScene::TownCenter => {
                w.demo_tree(TileCoord::new(124, 129))?;
                let tc = w.issue_build(
                    PlayerId(0),
                    &[worker],
                    BuildingKind::TownCenter,
                    TileCoord::new(128, 128),
                    false,
                )?;
                let b = w.building_mut(tc).ok_or(EconomyError::UnknownEntity)?;
                b.work_done = b.work_total - 3 * 60;
                b.hp = (u64::from(b.max_hp) * b.work_done / b.work_total) as u16;
            }
            BehaviourScene::Wolf => {
                w.insert_building(
                    PlayerId(0),
                    BuildingKind::House,
                    TileRect::from_xywh(129, 127, 2, 3),
                    true,
                )?;
                // Position setup only; the encounter begins through a hunt order.
                let wolf = w.spawn_animal(
                    AnimalKind::Wolf,
                    kingdom_position(TileCoord::new(125, 128))?,
                )?;
                w.issue_hunt(PlayerId(0), &[worker], wolf)?;
            }
            BehaviourScene::Deer => {
                w.spawn_animal(
                    AnimalKind::Deer,
                    kingdom_position(TileCoord::new(130, 129))?,
                )?;
            }
            BehaviourScene::Hunt => {
                let deer = w.spawn_animal(
                    AnimalKind::Deer,
                    kingdom_position(TileCoord::new(131, 129))?,
                )?;
                // One ordinary shot will kill this wounded deer, exposing the
                // transition to the retained death and carcass without a cut.
                w.strike_animal(deer, worker, 2);
                w.issue_hunt(PlayerId(0), &[worker], deer)?;
            }
            BehaviourScene::Bridge => {}
            BehaviourScene::Camps => {
                w.insert_building(
                    PlayerId(0),
                    BuildingKind::LumberCamp,
                    TileRect::from_xywh(123, 128, 2, 2),
                    true,
                )?;
                w.insert_building(
                    PlayerId(0),
                    BuildingKind::LumberCamp,
                    TileRect::from_xywh(140, 128, 2, 2),
                    true,
                )?;
                w.demo_tree(TileCoord::new(144, 129))?;
                let steward = w.spawn_kind(
                    PlayerId(0),
                    UnitKind::Steward,
                    kingdom_position(TileCoord::new(99, 100))?,
                )?;
                w.issue_garrison(PlayerId(0), steward, home)?;
            }
        }
        w.remember_visible_buildings();
        Ok((w, worker))
    }

    pub fn behaviour_environment(
        seed: Seed,
        scene: BehaviourScene,
    ) -> Result<
        (
            aoe_map::MapPackage,
            Vec<aoe_map::ElevationPage>,
            Vec<aoe_map::WaterPage>,
        ),
        EconomyError,
    > {
        kingdom_water_package(
            seed,
            15_360,
            if matches!(scene, BehaviourScene::Bridge) {
                behaviour_water()
            } else {
                Vec::new()
            },
        )
    }

    fn demo_tree(&mut self, near: TileCoord) -> Result<(), EconomyError> {
        let crate::Terrain::Map { generator, overlay } = &mut self.terrain else {
            return Err(EconomyError::InvalidResource);
        };
        for dx in 0..4 {
            let tile = TileCoord::new(near.x + dx, near.y);
            let node = ResourceNode::starting(tile, ResourceKind::Wood)
                .ok_or(EconomyError::InvalidResource)?;
            if overlay.insert_starting_resource(generator, node).is_ok() {
                self.resource_collision_changed();
                return Ok(());
            }
        }
        Err(EconomyError::InvalidResource)
    }
}

fn behaviour_water() -> Vec<aoe_map::WaterPage> {
    let mut pages = Vec::new();
    let mut axis = 64_usize;
    loop {
        let fine = pages.is_empty();
        pages.push(aoe_map::WaterPage {
            level: pages.len() as u8,
            x: 0,
            y: 0,
            width: axis as u8,
            height: axis as u8,
            ocean_coverage_percent: (0..axis * axis)
                .map(|i| if fine && i % axis == 32 { 100 } else { 0 })
                .collect(),
            inland_coverage_percent: vec![0; axis * axis],
        });
        if axis == 1 {
            break;
        }
        axis /= 2;
    }
    pages
}
