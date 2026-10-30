//! Explicit local showcase setup. All gameplay still uses the normal commands;
//! a one-Hz game clock under the server's 20-Hz ticker speeds this demo up 20x.
use crate::{BuildingKind, EconomyError, GameWorld, Stockpile};
use aoe_core::{EntityId, PlayerId, Seed, TileCoord, TileRect, WorldConfig};

impl GameWorld {
    /// Four-times-clock General showcase: a Castle, unclaimed role-balanced
    /// soldiers and a distant enemy group. Training and all orders stay real.
    pub fn general_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        use crate::UnitKind;
        let mut config =
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?;
        config.tick_hz = 5;
        let mut world = Self::new(config)?;
        let primary = world.initialize_village(PlayerId(0), TileCoord::new(80, 128))?;
        world.issue_move(
            primary,
            aoe_core::WorldPosition::from_tile_center(TileCoord::new(118, 138))
                .map_err(|_| EconomyError::InvalidFootprint)?,
        )?;
        world.complete_research(PlayerId(0), crate::Tech::FeudalAge, &mut Vec::new());
        world.complete_research(PlayerId(0), crate::Tech::CastleAge, &mut Vec::new());
        if let Some(player) = world.economy.players.get_mut(&PlayerId(0)) {
            player.stockpile = Stockpile {
                food: 10_000,
                wood: 10_000,
                gold: 10_000,
                stone: 10_000,
            };
        }
        world.insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(98, 124, 4, 4),
            true,
        )?;
        for n in 0..4 {
            for (kind, x) in [(UnitKind::Militia, 112), (UnitKind::Archer, 106)] {
                world.spawn_kind(
                    PlayerId(0),
                    kind,
                    aoe_core::WorldPosition::from_tile_center(TileCoord::new(x, 130 + n * 2))
                        .map_err(|_| EconomyError::InvalidFootprint)?,
                )?;
            }
        }
        for (kind, x, y) in [
            (UnitKind::BatteringRam, 102, 132),
            (UnitKind::Knight, 110, 125),
            (UnitKind::Knight, 110, 141),
            (UnitKind::Scout, 118, 130),
            (UnitKind::Scout, 118, 134),
        ] {
            world.spawn_kind(
                PlayerId(0),
                kind,
                aoe_core::WorldPosition::from_tile_center(TileCoord::new(x, y))
                    .map_err(|_| EconomyError::InvalidFootprint)?,
            )?;
        }
        for n in 0..5 {
            let id = world.spawn_kind(
                PlayerId(1),
                UnitKind::Militia,
                aoe_core::WorldPosition::from_tile_center(TileCoord::new(166, 128 + n * 2))
                    .map_err(|_| EconomyError::InvalidFootprint)?,
            )?;
            world.issue_stance(PlayerId(1), &[id], aoe_core::CombatStance::StandGround)?;
        }
        Ok((world, primary))
    }

    /// Local Castle economy fixture; production, garrison and management use
    /// exactly the same orders as a match. The existing demo clock runs 4x.
    pub fn vassal_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let (mut world, primary) = Self::military_demo(seed)?;
        world.complete_research(PlayerId(0), crate::Tech::CastleAge, &mut Vec::new());
        world.insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(139, 134, 4, 4),
            true,
        )?;
        Ok((world, primary))
    }

    /// Territory showcase on the four-times clock: two overlapping Castles, 40
    /// busy farmers between them, Vassal A already sheltered and Vassal B
    /// waiting outside. Garrisoning B splits the area; all orders stay real.
    pub fn vassal_territory_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        use crate::UnitKind;
        let (mut world, primary) = Self::military_demo(seed)?;
        world.complete_research(PlayerId(0), crate::Tech::CastleAge, &mut Vec::new());
        let mut vassals = Vec::new();
        for (x, y) in [(100, 100), (150, 100)] {
            let footprint = TileRect::from_xywh(x, y, 4, 4);
            let castle =
                world.insert_building(PlayerId(0), BuildingKind::Castle, footprint, true)?;
            let at = aoe_core::WorldPosition::from_tile_center(TileCoord::new(x + 2, y + 5))
                .map_err(|_| EconomyError::InvalidFootprint)?;
            let vassal = world.spawn_kind(PlayerId(0), UnitKind::Vassal, at)?;
            world.bind_vassal(vassal, castle, footprint);
            vassals.push((vassal, castle));
        }
        for i in 0..40 {
            let at = TileCoord::new(87 + 4 * (i % 20), 146 + 4 * (i / 20));
            let farm = world.insert_building(
                PlayerId(0),
                BuildingKind::Farm,
                TileRect::from_xywh(at.x, at.y, 3, 3),
                true,
            )?;
            let worker = world.spawn_villager(
                PlayerId(0),
                aoe_core::WorldPosition::from_tile_center(TileCoord::new(at.x + 3, at.y))
                    .map_err(|_| EconomyError::InvalidFootprint)?,
            )?;
            world.issue_farm(PlayerId(0), worker, farm)?;
        }
        let (vassal, castle) = vassals[0];
        world.issue_garrison(PlayerId(0), vassal, castle)?;
        Ok((world, primary))
    }

    /// Four-times-clock military playground; commands remain authoritative.
    pub fn military_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let mut config =
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?;
        config.tick_hz = 5;
        let mut world = Self::new(config)?;
        let primary = world.initialize_village(PlayerId(0), TileCoord::new(128, 128))?;
        if let Some(p) = world.economy.players.get_mut(&PlayerId(0)) {
            p.stockpile = Stockpile {
                food: 10000,
                wood: 10000,
                gold: 10000,
                stone: 10000,
            };
        }
        world.complete_research(PlayerId(0), crate::Tech::FeudalAge, &mut Vec::new());
        for (kind, x, y) in [
            (BuildingKind::Barracks, 121, 130),
            (BuildingKind::ArcheryRange, 128, 122),
            (BuildingKind::Stable, 131, 124),
            (BuildingKind::Blacksmith, 124, 126),
            (BuildingKind::House, 123, 135),
            (BuildingKind::House, 120, 135),
        ] {
            let r = kind.rules();
            world.insert_building(
                PlayerId(0),
                kind,
                TileRect::new(
                    TileCoord::new(x, y),
                    TileCoord::new(x + r.side, y + r.depth),
                ),
                true,
            )?;
        }
        for (kind, x, y) in [
            (crate::UnitKind::Militia, 126, 136),
            (crate::UnitKind::Scout, 128, 137),
        ] {
            world.spawn_kind(
                PlayerId(1),
                kind,
                aoe_core::WorldPosition::from_tile_center(TileCoord::new(x, y))
                    .map_err(|_| EconomyError::InvalidFootprint)?,
            )?;
        }
        Ok((world, primary))
    }

    pub fn feudal_demo(seed: Seed) -> Result<(Self, EntityId), EconomyError> {
        let mut config =
            WorldConfig::new(256, 256, seed).map_err(|_| EconomyError::InvalidFootprint)?;
        config.tick_hz = 1;
        let mut world = Self::new(config)?;
        let primary = world.initialize_village(PlayerId(0), TileCoord::new(128, 128))?;
        if let Some(player) = world.economy.players.get_mut(&PlayerId(0)) {
            player.stockpile = Stockpile {
                food: 10_000,
                wood: 10_000,
                gold: 10_000,
                stone: 10_000,
            };
        }
        for (kind, x, y) in [
            (BuildingKind::LumberCamp, 134, 128),
            (BuildingKind::Mill, 128, 136),
            (BuildingKind::Barracks, 120, 128),
        ] {
            let r = kind.rules();
            let rect = TileRect::new(
                TileCoord::new(x, y),
                TileCoord::new(x + r.side, y + r.depth),
            );
            world.validate_footprint(rect)?;
            world.insert_building(PlayerId(0), kind, rect, true)?;
        }
        Ok((world, primary))
    }
}
