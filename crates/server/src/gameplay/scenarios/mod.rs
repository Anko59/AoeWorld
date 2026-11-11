//! Server-side demo constructors using the ordinary authoritative world.
use super::{GameWorld, GameplayService, Seed};

#[cfg(test)]
mod tests;

impl GameplayService {
    pub fn behaviour_demo(
        seed: Seed,
        scene: aoe_simulation::BehaviourScene,
    ) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::behaviour_demo(seed, scene)?;
        let (package, _, _) = GameWorld::behaviour_environment(seed, scene)?;
        let metadata = crate::gameplay_map::map_metadata(&package);
        let service = Self::from_world(world, primary, Some(package.content_hash), Some(metadata));
        service.set_paused(true);
        Ok(service)
    }

    pub fn roads_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::roads_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn vassal_defense_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::vassal_defense_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    /// Seed the existing geographic world off the async runtime. Its package,
    /// page provider and immutable navigation remain the source for the film.
    pub(crate) async fn prepare_general_river_demo(&self) -> Result<Self, String> {
        if self.map_content_hash.is_none() {
            return Err("activate a source map first".to_owned());
        }
        let mut world = self.world.clone().write_owned().await;
        let (general, span, goal) =
            tokio::task::spawn_blocking(move || world.prepare_general_river_demo())
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
        tracing::info!(
            ?general,
            ?span,
            ?goal,
            "source river recording prepared; four-times clock"
        );
        let mut service = self.clone();
        service.sessions =
            std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::BTreeMap::new()));
        service.slots = vec![super::Slot {
            player: aoe_core::PlayerId(0),
            primary_unit_id: general,
            human: true,
            token: self.slots[0].token,
        }]
        .into();
        Ok(service)
    }

    /// A Vassal castle and a Town Center on opposite banks of a real river.
    pub(crate) async fn prepare_vassal_river_demo(&self) -> Result<Self, String> {
        if self.map_content_hash.is_none() {
            return Err("activate a source map first".to_owned());
        }
        let mut world = self.world.clone().write_owned().await;
        let (vassal, span) = tokio::task::spawn_blocking(move || world.prepare_vassal_river_demo())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        tracing::info!(?vassal, ?span, "vassal river fixture prepared");
        let mut service = self.clone();
        service.sessions =
            std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::BTreeMap::new()));
        service.slots = vec![super::Slot {
            player: aoe_core::PlayerId(0),
            primary_unit_id: vassal,
            human: true,
            token: self.slots[0].token,
        }]
        .into();
        Ok(service)
    }

    pub fn movement_lane_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::movement_lane_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }

    pub fn movement_recovery_demo(
        seed: Seed,
        acquired: bool,
    ) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::movement_recovery_demo(seed, acquired)?;
        Ok(Self::from_world(world, primary, None, None))
    }

    pub fn general_appearance_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::general_appearance_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn walk_crowd_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::walk_crowd_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn general_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::general_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn imperial_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::imperial_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn steward_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::steward_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn steward_territory_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::steward_territory_demo(seed)?;
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
    pub fn steward_farming_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::steward_farming_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn naval_battle_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::naval_battle_demo(seed)?;
        let (package, _, _) = GameWorld::naval_battle_environment(seed)?;
        let metadata = crate::gameplay_map::map_metadata(&package);
        Ok(Self::from_world(
            world,
            primary,
            Some(package.content_hash),
            Some(metadata),
        ))
    }
    pub fn steward_fishing_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::steward_fishing_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn steward_throughput_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::steward_throughput_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn steward_kingdom_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::steward_kingdom_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
    pub fn steward_stability_demo(seed: Seed) -> Result<Self, aoe_simulation::EconomyError> {
        let (world, primary) = GameWorld::steward_stability_demo(seed)?;
        Ok(Self::from_world(world, primary, None, None))
    }
}
