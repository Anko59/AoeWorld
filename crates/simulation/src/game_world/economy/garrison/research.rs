use crate::{GameWorld, Tech, TechEffect, UnitKind};
use aoe_core::PlayerId;

impl GameWorld {
    pub(crate) fn upgrade_garrisoned_units(&mut self, player: PlayerId, tech: Tech) {
        let units = self
            .economy
            .buildings
            .values_mut()
            .flat_map(|b| &mut b.garrison)
            .chain(
                self.economy
                    .ejections
                    .values_mut()
                    .flat_map(|(_, units)| units),
            );
        for g in units.filter(|g| g.unit.player == player) {
            let u = &mut g.unit;
            match tech.rules().effect {
                TechEffect::VillagerBonus { hp, .. } if u.kind == UnitKind::Villager => {
                    u.hp = u.hp.saturating_add(hp);
                    u.max_hp = u.max_hp.saturating_add(hp);
                }
                TechEffect::UpgradeMilitia if u.kind == UnitKind::Militia => {
                    u.kind = UnitKind::ManAtArms;
                    u.hp += 5;
                    u.max_hp += 5;
                }
                TechEffect::UpgradeUnit { from, to } if u.kind == from => {
                    let bonus = crate::unit_combat(to)
                        .hp
                        .saturating_sub(crate::unit_combat(from).hp);
                    u.kind = to;
                    u.hp += bonus;
                    u.max_hp += bonus;
                }
                _ => {}
            }
        }
    }
}
