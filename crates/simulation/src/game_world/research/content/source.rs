use super::*;

/// Native asset generation only; keep every balance number in its owning table.
#[cfg(any(not(target_arch = "wasm32"), feature = "content-source"))]
pub fn browser_content() -> Vec<i32> {
    use crate::{TechEffect, TechRequirement};
    let mut data = vec![
        MAGIC,
        crate::BUILDING_RULES.len() as i32,
        crate::TECH_RULES.len() as i32,
        crate::UNIT_COMBAT.len() as i32,
        crate::VILLAGE_RULES.villager_food_cost as i32,
    ];
    let price = |c: Stockpile| [c.food as i32, c.wood as i32, c.gold as i32, c.stone as i32];
    for r in crate::BUILDING_RULES {
        data.push(r.kind as i32);
        data.extend(price(r.cost));
        data.extend([
            r.side,
            r.depth,
            r.age as i32,
            i32::from(r.buildable),
            r.requires.map_or(0, |k| k as i32 + 1),
            r.trainable.iter().fold(0, |m, k| m | (1 << *k as u8)),
            i32::from(r.blocks),
            i32::from(r.max_hp),
            i32::from(r.population_support),
        ]);
    }
    for r in crate::TECH_RULES {
        data.extend([r.tech as i32, r.building as i32, r.age as i32]);
        data.extend(price(r.cost));
        let (tech, kinds, count) = match r.requires {
            TechRequirement::Technology(t) => (t as i32 + 1, 0, 0),
            TechRequirement::BuildingKinds { kinds, count } => (
                0,
                kinds.iter().fold(0, |m, k| m | (1 << *k as u8)),
                count as i32,
            ),
            _ => (0, 0, 0),
        };
        let capacity = match r.effect {
            TechEffect::VillagerEconomy {
                capacity_permyriad, ..
            } => capacity_permyriad,
            _ => 0,
        };
        data.extend([tech, kinds, count, r.seconds as i32, capacity as i32]);
    }
    for r in crate::UNIT_COMBAT {
        let training = r.training.unwrap_or(UnitTraining {
            cost: UnitCost::default(),
            seconds: 0,
        });
        let c = training.cost;
        let upgrades = crate::TECH_RULES
            .iter()
            .filter(|t| r.kind.upgraded(t.tech.bit()) != r.kind)
            .fold(0_u64, |m, t| m | t.tech.bit());
        data.extend([
            r.kind as i32,
            i32::from(r.hp),
            i32::from(r.attack),
            r.attack_kind as i32,
            i32::from(r.melee_armour),
            i32::from(r.pierce_armour),
            r.attack_period_centiseconds as i32,
            r.speed_hundredths.unwrap_or(0) as i32,
            i32::from(r.range),
            i32::from(r.min_range),
            i32::from(r.los),
            c.food as i32,
            c.wood as i32,
            c.gold as i32,
            c.stone as i32,
            training.seconds as i32,
            r.kind.required_age() as i32,
            r.kind.required_tech().map_or(0, |t| t as i32 + 1),
            upgrades as i32,
            (upgrades >> 32) as i32,
        ]);
    }
    data
}
