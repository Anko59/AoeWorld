//! Research: a building's queue spends time and resources on a technology
//! whose effect is applied on completion. Work per tick is bounded by the
//! building count (`WorldLimits::buildings`); only buildings that research are
//! touched.
use super::{EconomyError, GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId};

mod effects;
mod table;

pub use effects::villager_capacity;
pub(crate) use effects::villager_speed_percent;
pub use table::{Age, FEUDAL_BUILDINGS, TECH_RULES, Tech, TechEffect, TechRequirement, TechRules};

/// The technology a building is researching, in game ticks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResearchEntry {
    pub tech: Tech,
    pub progress_ticks: u64,
    pub total_ticks: u64,
}

impl GameWorld {
    pub fn has_researched(&self, player: PlayerId, tech: Tech) -> bool {
        self.economy
            .players
            .get(&player)
            .is_some_and(|state| state.researched & tech.bit() != 0)
    }

    /// Queue `tech` at `building`. The cost is paid now; the building does
    /// nothing else (no training) until the research ends or is cancelled.
    pub fn issue_research(
        &mut self,
        player: PlayerId,
        building: EntityId,
        tech: Tech,
    ) -> Result<(), EconomyError> {
        let rules = tech.rules();
        let site = self
            .economy
            .buildings
            .get(&building)
            .ok_or(EconomyError::UnknownEntity)?;
        if site.player != player {
            return Err(EconomyError::NotOwner);
        }
        if !site.is_complete() {
            return Err(EconomyError::UnderConstruction);
        }
        if site.kind != rules.building {
            return Err(EconomyError::NotResearchable);
        }
        if self.has_researched(player, tech) {
            return Err(EconomyError::AlreadyResearched);
        }
        if rules.requires == TechRequirement::Age(Age::Feudal) && !self.is_feudal(player) {
            return Err(EconomyError::RequiresFeudalAge);
        }
        let busy = site.research.is_some() || !site.training_queue.is_empty();
        let duplicate = self
            .economy
            .buildings
            .values()
            .any(|b| b.player == player && b.research.is_some_and(|r| r.tech == tech));
        if busy || duplicate {
            return Err(EconomyError::ResearchBusy);
        }
        if !self.requirement_met(player, rules.requires) {
            return Err(EconomyError::TechRequirementMissing);
        }
        let state = self
            .economy
            .players
            .get_mut(&player)
            .ok_or(EconomyError::NotOwner)?;
        let stock = &mut state.stockpile;
        let need = rules.cost;
        if stock.food < need.food
            || stock.wood < need.wood
            || stock.gold < need.gold
            || stock.stone < need.stone
        {
            return Err(EconomyError::InsufficientResources);
        }
        stock.food -= need.food;
        stock.wood -= need.wood;
        stock.gold -= need.gold;
        stock.stone -= need.stone;
        let total_ticks = u64::from(self.config.tick_hz) * u64::from(rules.seconds);
        if let Some(site) = self.economy.buildings.get_mut(&building) {
            site.research = Some(ResearchEntry {
                tech,
                progress_ticks: 0,
                total_ticks,
            });
        }
        Ok(())
    }

    /// Cancel the active research and refund all of its cost. Cancelling an
    /// idle building changes nothing.
    pub fn issue_cancel_research(
        &mut self,
        player: PlayerId,
        building: EntityId,
    ) -> Result<(), EconomyError> {
        let site = self
            .economy
            .buildings
            .get_mut(&building)
            .ok_or(EconomyError::UnknownEntity)?;
        if site.player != player {
            return Err(EconomyError::NotOwner);
        }
        let Some(entry) = site.research.take() else {
            return Ok(());
        };
        let refund = entry.tech.rules().cost;
        if let Some(state) = self.economy.players.get_mut(&player) {
            let stock = &mut state.stockpile;
            stock.food = stock.food.saturating_add(refund.food);
            stock.wood = stock.wood.saturating_add(refund.wood);
            stock.gold = stock.gold.saturating_add(refund.gold);
            stock.stone = stock.stone.saturating_add(refund.stone);
        }
        Ok(())
    }

    fn requirement_met(&self, player: PlayerId, requirement: TechRequirement) -> bool {
        match requirement {
            TechRequirement::None => true,
            TechRequirement::Age(Age::Dark) => true,
            TechRequirement::Age(Age::Feudal) => self.is_feudal(player),
            TechRequirement::BuildingKinds { count, kinds } => {
                kinds
                    .iter()
                    .filter(|kind| {
                        self.economy
                            .buildings
                            .values()
                            .any(|b| b.player == player && b.kind == **kind && b.is_complete())
                    })
                    .count()
                    >= count
            }
        }
    }

    /// Advance every research by one tick and apply those that finish.
    pub(super) fn advance_research(&mut self, changed: &mut Vec<EntityId>) {
        let mut finished = Vec::new();
        for (id, building) in &mut self.economy.buildings {
            let Some(entry) = &mut building.research else {
                continue;
            };
            entry.progress_ticks = entry
                .progress_ticks
                .saturating_add(1)
                .min(entry.total_ticks);
            if entry.progress_ticks == entry.total_ticks {
                finished.push((*id, building.player, entry.tech));
                building.research = None;
            }
        }
        for (_, player, tech) in finished {
            self.complete_research(player, tech, changed);
        }
    }

    /// Maximum hit points a new unit of `kind` starts with for `player`.
    pub(crate) fn researched_hp_bonus(&self, player: PlayerId, kind: UnitKind) -> u16 {
        self.villager_bonus(player, kind).0
    }

    /// Melee and pierce armour of a living unit, technologies included.
    pub fn unit_armour(&self, id: EntityId) -> Option<(i16, u16)> {
        let unit = self.unit(id)?;
        let base = self.unit_stats(id)?;
        let (_, melee, pierce) = self.villager_bonus(unit.player, unit.kind);
        Some((base.melee_armour + melee, base.pierce_armour + pierce))
    }
}
