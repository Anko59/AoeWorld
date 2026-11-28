use super::super::{GameplayService, SlotSetup};
use aoe_core::{PlayerId, Seed};
use aoe_simulation::{EconomyError, GameWorld};

impl GameplayService {
    pub(crate) fn diplomacy_demo(seed: Seed, pair: bool) -> Result<Self, EconomyError> {
        let (world, starts) = GameWorld::diplomacy_demo(seed)?;
        Ok(Self::diplomacy_scene(world, starts, pair))
    }

    pub(crate) fn diplomacy_conquest_demo(seed: Seed) -> Result<Self, EconomyError> {
        let (world, starts) = GameWorld::diplomacy_conquest_demo(seed)?;
        Ok(Self::diplomacy_scene(world, starts, false))
    }

    fn diplomacy_scene(world: GameWorld, starts: [aoe_core::EntityId; 2], pair: bool) -> Self {
        let slots = starts
            .into_iter()
            .enumerate()
            .map(|(index, primary_unit_id)| SlotSetup {
                player: PlayerId(index as u16),
                primary_unit_id,
                human: index == 0 || pair,
            })
            .collect();
        let service = Self::from_named_world_slots(
            world,
            slots,
            None,
            None,
            [(PlayerId(1), "Lyon".to_owned())].into(),
        );
        service.set_paused(true);
        service
    }
}
