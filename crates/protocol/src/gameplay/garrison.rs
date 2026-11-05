use super::{BuildingKind, ConstructionState, GameplayBuildingState, UnitKind};
use aoe_core::EntityId;
use serde::{Deserialize, Deserializer, Serialize};

/// Owner-visible occupants. They have no map position or targetable unit state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GarrisonUnitState {
    pub id: EntityId,
    pub kind: UnitKind,
    pub hp: u16,
    pub max_hp: u16,
    /// Owner-only economy report of a garrisoned, active Steward.
    pub steward: Option<StewardReport>,
    pub vassal: Option<VassalReport>,
}

/// Owner-only finite fortification progress and defence alert.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VassalReport {
    pub villagers: u8,
    pub military: u8,
    pub development: bool,
    pub developed: bool,
    pub alert: bool,
    pub next: Option<BuildingKind>,
    /// Building, reserve, builders, blocked, paused, developed, age.
    pub wait: u8,
    pub completed: u16,
    pub total: u16,
    pub responding: u16,
}

/// What the Steward's domain is doing and, if it is not growing, why not.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StewardReport {
    pub weights: [u8; 4],
    pub cap: u8,
    pub training: bool,
    pub managed: u16,
    pub residents: u16,
    pub wait: StewardWait,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StewardWait {
    Growing,
    Census,
    Full,
    NoTownCenter,
    Food,
    Population,
    Paused,
}

impl BuildingKind {
    pub const fn garrison_capacity(self) -> u8 {
        match self {
            Self::TownCenter => 15,
            Self::Castle => 20,
            Self::WatchTower => 5,
            _ => 0,
        }
    }
    pub const fn admits(self, kind: UnitKind) -> bool {
        (!matches!(kind, UnitKind::Steward) || matches!(self, Self::TownCenter))
            && !kind.is_naval()
            && self.garrison_capacity() > 0
            && (matches!(self, Self::Castle)
                || matches!(
                    kind,
                    UnitKind::Villager
                        | UnitKind::Steward
                        | UnitKind::Militia
                        | UnitKind::ManAtArms
                        | UnitKind::Spearman
                        | UnitKind::Archer
                        | UnitKind::Skirmisher
                        | UnitKind::LongSwordsman
                        | UnitKind::Pikeman
                        | UnitKind::Crossbowman
                        | UnitKind::EliteSkirmisher
                        | UnitKind::TwoHandedSwordsman
                        | UnitKind::Champion
                        | UnitKind::Halberdier
                        | UnitKind::Arbalester
                ))
    }
}

pub(super) fn bounded_garrison<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<GarrisonUnitState>, D::Error> {
    super::economy::bounded::<D, _, 20>(d)
}

pub(super) fn valid(b: &GameplayBuildingState) -> bool {
    b.garrison.len() <= usize::from(b.kind.garrison_capacity())
        && (b.garrison.is_empty() || b.construction == ConstructionState::Complete)
        && b.garrison.windows(2).all(|pair| pair[0].id < pair[1].id)
        && b.garrison
            .iter()
            .all(|u| u.id != b.id && b.kind.admits(u.kind) && u.hp > 0 && u.hp <= u.max_hp)
        && b.garrison
            .iter()
            .all(|u| steward_valid(u) && vassal_valid(u))
}

/// A Steward report belongs to a Steward and stays within the managed bounds.
fn steward_valid(u: &GarrisonUnitState) -> bool {
    u.steward.is_none_or(|r| {
        u.kind == UnitKind::Steward
            && r.managed <= 100
            && r.managed <= r.residents
            && r.cap <= 100
            && r.weights.iter().all(|w| *w <= 100)
            && r.weights.iter().any(|w| *w > 0)
    })
}

fn vassal_valid(u: &GarrisonUnitState) -> bool {
    u.vassal.is_none_or(|r| {
        u.kind == UnitKind::Vassal
            && r.villagers <= 20
            && r.military <= 20
            && r.wait <= 6
            && r.completed <= r.total
            && (!r.developed || r.completed == r.total)
    })
}

/// Keeps the occupants `valid` accepts, in order and within capacity, so one
/// bad occupant costs only itself.
#[cfg(feature = "server-repair")]
pub(super) fn repair(b: &mut GameplayBuildingState) {
    if b.construction != ConstructionState::Complete {
        b.garrison.clear();
        return;
    }
    let (id, kind) = (b.id, b.kind);
    for u in &mut b.garrison {
        if !vassal_valid(u) {
            u.vassal = None;
        }
        if !steward_valid(u) {
            u.steward = None;
        }
    }
    b.garrison
        .retain(|u| u.id != id && kind.admits(u.kind) && u.hp > 0 && u.hp <= u.max_hp);
    b.garrison.sort_by_key(|u| u.id);
    b.garrison.dedup_by_key(|u| u.id);
    b.garrison.truncate(usize::from(kind.garrison_capacity()));
}
