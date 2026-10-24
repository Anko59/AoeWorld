//! A reason accompanies every disabled research/training action. Numeric codes
//! feed the DOM alongside enable flags; costs and prerequisites use game tables.
use super::{rules, stock};
use aoe_core::PlayerId;
use aoe_protocol::{
    Age, ConstructionState, GameplayBuildingState, GameplayPlayerState, Stockpile, Tech, UnitKind,
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
    RequiresManAtArms,
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
    completed: u16,
    duplicate: bool,
    r: &aoe_simulation::TechRules,
) -> Availability {
    if r.building as u8 != b.kind as u8 {
        return Availability::Unavailable;
    }
    if let Some(reason) = owned_complete(b, owner) {
        return reason;
    }
    let Some(p) = player else {
        return Availability::NotOwner;
    };
    if p.researched & r.tech.bit() != 0 {
        return Availability::Researched;
    }
    if r.requires == aoe_simulation::TechRequirement::Age(aoe_simulation::Age::Feudal)
        && p.age != Age::Feudal
    {
        return Availability::RequiresFeudal;
    }
    if b.research.is_some() || duplicate {
        return Availability::ResearchBusy;
    }
    if !b.training_queue.is_empty() {
        return Availability::TrainingBusy;
    }
    if let aoe_simulation::TechRequirement::BuildingKinds { count, kinds } = r.requires
        && kinds
            .iter()
            .filter(|k| completed & (1 << **k as u8) != 0)
            .count()
            < count
    {
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
    let native = aoe_simulation::UNIT_COMBAT[kind as usize].kind;
    if !rules(b.kind).trainable.contains(&native) {
        return Availability::Unavailable;
    }
    if let Some(reason) = owned_complete(b, owner) {
        return reason;
    }
    let Some(p) = player else {
        return Availability::NotOwner;
    };
    if matches!(
        kind,
        UnitKind::Archer
            | UnitKind::Skirmisher
            | UnitKind::Spearman
            | UnitKind::Scout
            | UnitKind::ManAtArms
    ) && p.age != Age::Feudal
    {
        return Availability::RequiresFeudal;
    }
    if kind == UnitKind::ManAtArms && p.researched & Tech::ManAtArms.bit() == 0 {
        return Availability::RequiresManAtArms;
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
            food: aoe_simulation::VILLAGE_RULES.villager_food_cost,
            ..Default::default()
        }
    } else {
        let Some(t) = aoe_simulation::GameWorld::unit_training(native) else {
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
            if player.is_some_and(|p| p.researched & Tech::ManAtArms.bit() != 0) {
                UnitKind::ManAtArms
            } else {
                UnitKind::Militia
            }
        }
        aoe_protocol::BuildingKind::Dock => UnitKind::FishingShip,
        _ => UnitKind::Villager,
    }
}
