//! Server-side demo constructors using the ordinary authoritative world.
use super::{GameWorld, GameplayService, Seed};

#[cfg(test)]
mod tests;

impl GameplayService {
    pub fn general_appearance_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::general_appearance_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn general_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::general_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn vassal_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::vassal_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn vassal_territory_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::vassal_territory_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn military_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::military_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    /// Local Feudal showcase setup; the ordinary command and wire paths apply.
    pub fn feudal_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::feudal_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn vassal_farming_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::vassal_farming_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn vassal_kingdom_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::vassal_kingdom_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
}
