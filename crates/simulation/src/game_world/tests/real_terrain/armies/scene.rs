use super::*;
use crate::game_world::terrain_fixtures::{Fixture, build};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Shape {
    River,
    Forest,
    Pass,
    /// The river again, with the goal on the near bank 150 tiles south of the
    /// start: a lateral slot 20 tiles wide of the goal lies across the water.
    Bank,
}

impl Shape {
    pub fn fixture(self) -> &'static str {
        match self {
            Self::River | Self::Bank => "army-river",
            Self::Forest => "army-forest",
            Self::Pass => "army-pass",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Cell {
    pub shape: Shape,
    pub kind: UnitKind,
    pub count: usize,
    pub general: bool,
}

/// The owner's late army: four infantry, three heavy and two light cavalry,
/// one ram. `UnitKind::General` as a cell's kind stands for this mix.
const MIX: [UnitKind; 10] = [
    UnitKind::Champion,
    UnitKind::Paladin,
    UnitKind::Hussar,
    UnitKind::Champion,
    UnitKind::Paladin,
    UnitKind::SiegeRam,
    UnitKind::Champion,
    UnitKind::Paladin,
    UnitKind::Hussar,
    UnitKind::Champion,
];

/// Soldiers one General can lead, by what squads allow for this arm.
pub(super) fn per_general(kind: UnitKind) -> usize {
    match kind {
        UnitKind::General => 100,
        k if k.is_siege() => 50,
        UnitKind::Paladin | UnitKind::Knight | UnitKind::Cavalier | UnitKind::Hussar => 100,
        _ => 200,
    }
}

fn spacing(kind: UnitKind) -> i32 {
    match kind {
        UnitKind::General => 2,
        k if k.is_siege() => 3,
        UnitKind::Paladin | UnitKind::Knight | UnitKind::Cavalier | UnitKind::Hussar => 2,
        _ => 1,
    }
}

fn tile(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

/// Goal 270 tiles east of the start, past the obstacle at x = 150.
pub(super) const GOAL: (i32, i32) = (300, 100);
pub(super) const OBSTACLE_END: i32 = 170;
pub(super) const BANK_GOAL: (i32, i32) = (132, 300);

pub(super) struct Scene {
    pub fixture: Fixture,
    pub soldiers: Vec<EntityId>,
    pub generals: Vec<EntityId>,
    pub goal: WorldPosition,
}

impl Scene {
    pub fn new(cell: Cell) -> Self {
        let mut fixture = build(cell.shape.fixture());
        let (mut soldiers, mut generals) = (Vec::new(), Vec::new());
        let gap = spacing(cell.kind);
        let chunk = if cell.general {
            per_general(cell.kind)
        } else {
            cell.count
        };
        let mut left = cell.count;
        let mut block_y = 100;
        let mut spawned = Vec::new();
        while left > 0 {
            let n = left.min(chunk);
            let side = (n as f64).sqrt().ceil() as i32;
            let start_y = block_y - side * gap / 2;
            for i in 0..n as i32 {
                let at = tile(30 + (i % side) * gap, start_y + (i / side) * gap);
                let kind = if cell.kind == UnitKind::General {
                    MIX[i as usize % MIX.len()]
                } else {
                    cell.kind
                };
                soldiers.push(fixture.world.spawn_kind(PlayerId(0), kind, at).unwrap());
            }
            spawned.push((30 + side * gap / 2, block_y));
            left -= n;
            block_y += side * gap + 3;
        }
        if cell.general {
            for (x, y) in spawned {
                let id = fixture
                    .world
                    .spawn_kind(PlayerId(0), UnitKind::General, tile(x, y))
                    .unwrap();
                generals.push(id);
            }
        }
        Self {
            fixture,
            soldiers,
            generals,
            goal: if cell.shape == Shape::Bank {
                tile(BANK_GOAL.0, BANK_GOAL.1)
            } else {
                tile(GOAL.0, GOAL.1)
            },
        }
    }

    /// What a player's right click on the whole army does.
    pub fn order(&mut self) -> Result<(), crate::EconomyError> {
        if self.generals.is_empty() {
            return self.fixture.world.issue_formation_move(
                PlayerId(0),
                &self.soldiers,
                aoe_core::Formation::Line,
                self.goal,
                false,
                false,
            );
        }
        for &g in &self.generals {
            self.fixture
                .world
                .issue_move(g, self.goal)
                .map_err(crate::EconomyError::Movement)?;
        }
        Ok(())
    }
}
