//! Menu rules sent once when the client binds its gameplay callback.
use super::super::orders;
use crate::building_ui::{self, availability};
use aoe_protocol::{Stockpile, Tech, UnitKind};

fn stock_values(stock: Stockpile) -> [f64; 4] {
    [stock.food, stock.wood, stock.gold, stock.stone].map(f64::from)
}

// Successors share the slot of their earliest entry in the ordered catalog.
fn line_root(mut index: usize, parent: impl Fn(usize) -> Option<usize>) -> usize {
    for _ in Tech::ALL {
        let Some(previous) = parent(index) else { break };
        index = previous;
    }
    index
}

pub(super) fn values() -> Vec<f64> {
    let rules = building_ui::content::rules();
    let mut values = Vec::new();
    for (building, rule) in rules.buildings.iter().zip(aoe_simulation::BUILDING_RULES) {
        let drop_off = rule
            .accepts
            .into_iter()
            .enumerate()
            .fold(0, |mask, (index, accepted)| {
                mask | u8::from(accepted) << index
            });
        values.extend([
            f64::from(building.age),
            f64::from(u8::from(building.buildable)),
            building.requires.map_or(-1.0, f64::from),
            f64::from(building.population_support),
            f64::from(drop_off),
        ]);
        values.extend(stock_values(building_ui::stock(building.cost)));
        let attack = aoe_simulation::building_combat(rule.kind).attack;
        values.extend(attack.map_or([0.0; 3], |stats| {
            [stats.attack, stats.min_range.into(), stats.range.into()].map(f64::from)
        }));
    }
    for (index, tech) in rules.techs.iter().enumerate() {
        let slot = match Tech::ALL[index] {
            Tech::CastleAge | Tech::ImperialAge => Tech::FeudalAge as usize,
            _ => line_root(index, |i| {
                let parent = usize::from(rules.techs[i].requires_tech?);
                (rules.techs.get(parent)?.building == tech.building).then_some(parent)
            }),
        };
        values.extend([f64::from(tech.building), slot as f64]);
        values.extend(stock_values(building_ui::stock(tech.cost)));
    }
    for kind in UnitKind::ALL {
        let slot = line_root(kind as usize, |i| {
            let units = aoe_simulation::UnitKind::ALL;
            let next = units[i];
            let upgrade = next.required_tech()?.bit();
            units
                .iter()
                .position(|&unit| unit != next && unit.upgraded(upgrade) == next)
        });
        values.extend([
            f64::from(u8::from(orders::can_form(kind))),
            f64::from(u8::from(orders::can_set_stance(kind))),
            slot as f64,
        ]);
        values.extend(stock_values(
            availability::training_cost(kind).unwrap_or_default(),
        ));
    }
    values
}
