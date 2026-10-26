//! A reason accompanies every disabled research/training action. Numeric codes
//! feed the DOM alongside enable flags; costs and prerequisites use game tables.
use super::{rules, stock};
use aoe_core::PlayerId;
use aoe_protocol::{
    ConstructionState, GameplayBuildingState, GameplayPlayerState, Stockpile, Tech, UnitKind,
};

#[derive(Clone, Copy)]
#[repr(u8)]
pub(crate) enum Availability {
    Unavailable,
    Ready,
    RequiresFeudal,
    Researched,
    ResearchBusy,
    TrainingBusy,
    Food,
    Wood,
    Gold,
    Stone,
    Population,
    QueueFull,
    Construction,
    Requirement,
    NotOwner,
    RequiresCastle,
}
impl Availability {
    pub(crate) fn number(self) -> f64 {
        self as u8 as f64
    }
}

fn cost(have: Stockpile, need: Stockpile) -> Availability {
    if have.food < need.food {
        Availability::Food
    } else if have.wood < need.wood {
        Availability::Wood
    } else if have.gold < need.gold {
        Availability::Gold
    } else if have.stone < need.stone {
        Availability::Stone
    } else {
        Availability::Ready
    }
}

fn owned_complete(b: &GameplayBuildingState, owner: PlayerId) -> Option<Availability> {
    if b.player != owner {
        Some(Availability::NotOwner)
    } else if b.construction != ConstructionState::Complete {
        Some(Availability::Construction)
    } else {
        None
    }
}

pub(crate) fn research(
    b: &GameplayBuildingState,
    player: Option<&GameplayPlayerState>,
    owner: PlayerId,
    completed: u32,
    duplicate: bool,
    r: &aoe_simulation::BrowserTech,
) -> Availability {
    if r.building != b.kind as u8 {
        return Availability::Unavailable;
    }
    if let Some(reason) = owned_complete(b, owner) {
        return reason;
    }
    let Some(p) = player else {
        return Availability::NotOwner;
    };
    if p.researched & (1_u64 << r.tech) != 0 {
        return Availability::Researched;
    }
    if (p.age as u8) < r.age {
        return if r.age == aoe_simulation::Age::Castle as u8 {
            Availability::RequiresCastle
        } else {
            Availability::RequiresFeudal
        };
    }
    if let Some(tech) = r.requires_tech
        && p.researched & (1_u64 << tech) == 0
    {
        return Availability::Requirement;
    }
    if b.research.is_some() || duplicate {
        return Availability::ResearchBusy;
    }
    if !b.training_queue.is_empty() {
        return Availability::TrainingBusy;
    }
    if (completed & r.building_kinds).count_ones() < u32::from(r.building_count) {
        return Availability::Requirement;
    }
    cost(p.stockpile, stock(r.cost))
}

pub(crate) fn training(
    b: &GameplayBuildingState,
    player: Option<&GameplayPlayerState>,
    owner: PlayerId,
    reserved: usize,
    kind: UnitKind,
) -> Availability {
    let unit = &super::content::rules().units[kind as usize];
    if !rules(b.kind).trains(kind as u8) {
        return Availability::Unavailable;
    }
    if let Some(reason) = owned_complete(b, owner) {
        return reason;
    }
    let Some(p) = player else {
        return Availability::NotOwner;
    };
    if (p.age as u8) < unit.age {
        return if unit.age == aoe_simulation::Age::Castle as u8 {
            Availability::RequiresCastle
        } else {
            Availability::RequiresFeudal
        };
    }
    if unit
        .requires_tech
        .is_some_and(|t| p.researched & (1_u64 << t) == 0)
    {
        return Availability::Requirement;
    }
    if unit.superseded(p.researched) {
        return Availability::Unavailable;
    }
    if b.research.is_some() {
        return Availability::ResearchBusy;
    }
    if b.training_queue.len() >= aoe_protocol::MAX_TRAINING_QUEUE {
        return Availability::QueueFull;
    }
    if usize::from(p.population_used) + reserved >= usize::from(p.population_cap) {
        return Availability::Population;
    }
    let need = if kind == UnitKind::Villager {
        Stockpile {
            food: super::content::rules().villager_food,
            ..Default::default()
        }
    } else {
        let Some(t) = unit.combat.training else {
            return Availability::Unavailable;
        };
        Stockpile {
            food: t.cost.food,
            wood: t.cost.wood,
            gold: t.cost.gold,
            stone: t.cost.stone,
        }
    };
    cost(p.stockpile, need)
}

pub(crate) fn primary_training(
    b: &GameplayBuildingState,
    player: Option<&GameplayPlayerState>,
) -> UnitKind {
    match b.kind {
        aoe_protocol::BuildingKind::Barracks => {
            if player.is_some_and(|p| p.researched & Tech::LongSwordsman.bit() != 0) {
                UnitKind::LongSwordsman
            } else if player.is_some_and(|p| p.researched & Tech::ManAtArms.bit() != 0) {
                UnitKind::ManAtArms
            } else {
                UnitKind::Militia
            }
        }
        aoe_protocol::BuildingKind::Dock => UnitKind::FishingShip,
        _ => UnitKind::Villager,
    }
}
