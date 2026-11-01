//! One deterministic map/command fixture for tests, profiling and local filming.
use crate::game_world::Terrain;
use crate::{BuildingKind, GameWorld, UnitKind};
use aoe_core::{
    EntityId, Formation, PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition,
};
use aoe_map::*;
use std::sync::Arc;
mod pages;
pub(crate) use pages::flat_fixture;

mod environment;
use environment::{at, base, block, mixed, trees};

pub struct Fixture {
    pub world: GameWorld,
    pub primary: EntityId,
    pub package: MapPackage,
    pub elevation: Vec<ElevationPage>,
    pub water: Vec<WaterPage>,
    pub vegetation: Vec<PotentialBiomePage>,
    pub hydrology: Option<HydrologyEvidencePage>,
    pub cover: Option<ModernLandCoverPage>,
    pub ids: Vec<EntityId>,
    pub commands: Vec<Command>,
}

#[derive(Debug)]
pub enum Command {
    Group(Vec<EntityId>, WorldPosition),
    Move(EntityId, WorldPosition),
    General(EntityId, WorldPosition),
}

impl Fixture {
    /// Bind constructor geometry, page evidence, actors and commands without
    /// depending on the engine's changing internal canonical state format.
    pub fn fingerprint(&self) -> String {
        let mut hash = blake3::Hasher::new();
        for bytes in [
            include_bytes!("mod.rs").as_slice(),
            include_bytes!("environment.rs").as_slice(),
            include_bytes!("pages.rs").as_slice(),
            self.package.content_hash.as_slice(),
        ] {
            hash.update(bytes);
        }
        hash.update(
            format!(
                "{:?}{:?}{:?}{:?}{:?}",
                self.elevation, self.water, self.vegetation, self.hydrology, self.cover
            )
            .as_bytes(),
        );
        hash.update(format!("{:?}{:?}", self.primary, self.commands).as_bytes());
        for u in self.world.units() {
            hash.update(format!("{:?}{:?}{:?}{:?}", u.id, u.player, u.kind, u.position).as_bytes());
        }
        hash.finalize().to_hex().to_string()
    }

    /// Observation starts before this call, including admission and General cadence.
    pub fn issue(&mut self) {
        for command in &self.commands {
            match command {
                Command::Group(ids, goal) => {
                    self.world
                        .issue_formation_move(
                            PlayerId(0),
                            ids,
                            Formation::Line,
                            *goal,
                            false,
                            false,
                        )
                        .unwrap();
                }
                Command::Move(id, goal) => {
                    self.world.issue_move(*id, *goal).unwrap();
                }
                Command::General(id, goal) => {
                    // Every follower was adopted at spawn: all ids but the General.
                    assert_eq!(
                        self.world.general_status(*id).unwrap().army_size,
                        u16::try_from(self.ids.len() - 1).unwrap()
                    );
                    self.world.issue_move(*id, *goal).unwrap();
                }
            }
        }
    }

    fn group(&mut self, x: i32, y: i32, count: usize, goal: WorldPosition) {
        let ids = mixed(&mut self.world, x, y, count);
        self.ids.extend_from_slice(&ids);
        self.commands.push(Command::Group(ids, goal));
    }

    /// A General at `general` adopts a 100-soldier block at `block_at`, then
    /// receives an ordinary move order toward `goal`.
    fn general_army(&mut self, general: (i32, i32), block_at: (i32, i32), goal: WorldPosition) {
        let mut ids = block(&mut self.world, block_at.0, block_at.1, 100);
        let general = self
            .world
            .spawn_kind(PlayerId(0), UnitKind::General, at(general.0, general.1))
            .unwrap();
        ids.push(general);
        self.ids.extend(ids);
        self.commands.push(Command::General(general, goal));
    }

    fn move_unit(&mut self, kind: UnitKind, from: WorldPosition, to: WorldPosition) {
        let id = self.world.spawn_kind(PlayerId(0), kind, from).unwrap();
        self.ids.push(id);
        self.commands.push(Command::Move(id, to));
    }
}

/// No warm-up or commands are hidden inside fixture construction.
pub fn build(name: &str) -> Fixture {
    let mut f = base(name);
    match name {
        "river" => f.group(35, 43, 40, at(145, 48)),
        "forest" => {
            trees(&mut f.world, TileRect::from_xywh(75, 15, 12, 31));
            trees(&mut f.world, TileRect::from_xywh(75, 52, 12, 55));
            f.group(35, 43, 40, at(135, 48));
        }
        "town" => {
            for x in [68, 82, 96] {
                for y in [24, 40, 56, 72] {
                    for (kind, dx, dy, complete) in [
                        (BuildingKind::Barracks, 0, 0, true),
                        (BuildingKind::House, 4, 0, false),
                        (BuildingKind::Farm, 0, 4, true),
                    ] {
                        let r = kind.rules();
                        f.world
                            .insert_building(
                                PlayerId(0),
                                kind,
                                TileRect::from_xywh(x + dx, y + dy, r.side, r.depth),
                                complete,
                            )
                            .unwrap();
                    }
                }
            }
            f.group(30, 43, 40, at(145, 48));
        }
        "crossing" => {
            trees(&mut f.world, TileRect::from_xywh(65, 10, 30, 20));
            trees(&mut f.world, TileRect::from_xywh(65, 80, 30, 20));
            f.group(35, 43, 40, at(125, 50));
            f.group(115, 43, 40, at(30, 50));
        }
        "long" => {
            for (x, y) in [(310, 65), (600, 95), (850, 50)] {
                trees(&mut f.world, TileRect::from_xywh(x, y, 45, 24));
            }
            f.ids = mixed(&mut f.world, 35, 90, 99);
            let general = f
                .world
                .spawn_kind(PlayerId(0), UnitKind::General, at(52, 106))
                .unwrap();
            f.ids.push(general);
            f.commands.push(Command::General(general, at(1045, 100)));
        }
        "bridge" => {
            let span =
                aoe_core::BridgeSpan::new(TileCoord::new(83, 65), TileCoord::new(92, 71)).unwrap();
            f.world
                .insert_building_shape(
                    PlayerId(0),
                    BuildingKind::Bridge,
                    span.footprint(),
                    true,
                    Some(span),
                )
                .unwrap();
            if let Terrain::Map { overlay, .. } = &mut f.world.terrain {
                overlay.add_bridge(span).unwrap();
            }
            for tile in span.tiles() {
                f.world.economy.blocked_tiles.remove(&tile);
            }
            f.world.collision_opened(span.footprint());
            f.move_unit(UnitKind::Villager, at(80, 63), at(95, 73));
            f.move_unit(UnitKind::Villager, at(95, 73), at(80, 63));
        }
        // A General's 100 followers (fixture-scale, one-tile block spacing).
        "general-river" => f.general_army((26, 43), (30, 38), at(145, 48)),
        "general-forest" => {
            trees(&mut f.world, TileRect::from_xywh(75, 15, 12, 31));
            trees(&mut f.world, TileRect::from_xywh(75, 52, 12, 55));
            f.general_army((26, 43), (30, 38), at(135, 48));
        }
        // The General stands on the east bank, the followers on the west bank.
        "general-far-bank" => f.general_army((96, 43), (74, 38), at(150, 48)),
        "siege" => {
            f.world
                .insert_building(
                    PlayerId(0),
                    BuildingKind::Barracks,
                    TileRect::from_xywh(60, 48, 3, 3),
                    false,
                )
                .unwrap();
            f.move_unit(UnitKind::Mangonel, at(40, 49), at(85, 49));
        }
        "gap" => {
            trees(&mut f.world, TileRect::from_xywh(40, 49, 30, 1));
            trees(&mut f.world, TileRect::from_xywh(40, 51, 30, 1));
            f.move_unit(UnitKind::Villager, at(43, 50), at(66, 50));
            f.move_unit(UnitKind::Villager, at(66, 50), at(43, 50));
        }
        _ => panic!("unknown fixture {name}"),
    }
    f.primary = f
        .commands
        .iter()
        .find_map(|c| match c {
            Command::General(id, _) => Some(*id),
            _ => None,
        })
        .unwrap_or(f.ids[0]);
    f
}
