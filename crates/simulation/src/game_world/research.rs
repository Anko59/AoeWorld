//! Research: a building's queue spends time and resources on a technology
//! whose effect is applied on completion. Work per tick is bounded by the
//! building count (`WorldLimits::buildings`); only buildings that research are
//! touched.
use super::{EconomyError, GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId};

mod castle;
mod content;
mod effects;
mod table;
mod upgrades;

#[cfg(any(not(target_arch = "wasm32"), feature = "content-source"))]
pub use content::browser_content;
pub use content::{BrowserBuilding, BrowserContent, BrowserTech, BrowserUnit};

pub use effects::villager_capacity;
pub(crate) use effects::{carry_capacity, villager_speed_percent};
pub use table::{Age, FEUDAL_BUILDINGS, TECH_RULES, Tech, TechEffect, TechRequirement, TechRules};

/// The technology a building is researching, in game ticks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResearchEntry {
    pub tech: Tech,
    pub progress_ticks: u64,
    pub total_ticks: u64,
}

impl GameWorld {
    pub(crate) fn require_age(&self, player: PlayerId, age: Age) -> Result<(), EconomyError> {
        if self.player_economy(player).is_some_and(|p| p.age >= age) {
            return Ok(());
        }
        Err(if age == Age::Castle {
            EconomyError::RequiresCastleAge
        } else {
            EconomyError::RequiresFeudalAge
        })
    }

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
        self.require_age(player, rules.age)?;
        let busy = site.research.is_some() || !site.training_queue.is_empty();
        let duplicate = self
            .economy
            .active_research
            .iter()
            .filter_map(|id| self.economy.buildings.get(id))
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
            self.economy.active_research.insert(building);
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
        self.economy.active_research.remove(&building);
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
            TechRequirement::Age(age) => self.require_age(player, age).is_ok(),
            TechRequirement::Technology(tech) => self.has_researched(player, tech),
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
        #[cfg(test)]
        {
            self.economy.tick_work.research_buildings = self.economy.active_research.len();
        }
        let mut finished = Vec::new();
        let active = self
            .economy
            .active_research
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for id in active {
            let Some(building) = self.economy.buildings.get_mut(&id) else {
                continue;
            };
            let Some(entry) = &mut building.research else {
                continue;
            };
            entry.progress_ticks = entry
                .progress_ticks
                .saturating_add(1)
                .min(entry.total_ticks);
            if entry.progress_ticks == entry.total_ticks {
                finished.push((id, building.player, entry.tech));
                building.research = None;
                self.economy.active_research.remove(&id);
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
