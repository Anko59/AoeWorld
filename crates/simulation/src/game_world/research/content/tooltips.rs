//! Render the simulation's research effects into the browser's startup content.
use super::super::{
    ArmorClass, AttackBonus, BuildingGroup, ConversionTarget, RuleEffect, TechEffect, TechRules,
    TechTarget,
};
use crate::game_world::combat::{conversion_chance, researched_stats};
use crate::{Age, BuildingKind, TECH_RULES, Tech, UnitKind};

pub fn browser_research_tooltips() -> Vec<String> {
    TECH_RULES.iter().map(tooltip).collect()
}

fn tooltip(rule: &TechRules) -> String {
    let mut parts = vec![describe(rule)];
    let unlocks: Vec<_> = UnitKind::ALL
        .iter()
        .copied()
        .filter(|unit| unit.required_tech() == Some(rule.tech))
        .map(name)
        .collect();
    if !unlocks.is_empty() {
        parts.push(format!("Unlocks {}", unlocks.join(", ")));
    }
    let cost = rule.cost;
    for (amount, resource) in [
        (cost.food, "food"),
        (cost.wood, "wood"),
        (cost.gold, "gold"),
        (cost.stone, "stone"),
    ] {
        if amount != 0 {
            parts.push(format!("{amount} {resource}"));
        }
    }
    parts.push(format!("{} s", rule.seconds));
    parts.join(" · ")
}

fn describe(rule: &TechRules) -> String {
    use TechEffect::*;
    match rule.effect {
        Rule(effect) => rule_effect(rule.tech, effect),
        UpgradeMilitia => upgrade(UnitKind::Militia, UnitKind::ManAtArms),
        UpgradeUnit { from, to } => upgrade(from, to),
        UpgradeShips(pairs) => pairs
            .iter()
            .map(|&(from, to)| upgrade(from, to))
            .collect::<Vec<_>>()
            .join(", "),
        UpgradeBuilding { from, to } => upgrade(from, to),
        VillagerBonus {
            hp,
            melee_armour,
            pierce_armour,
        } => {
            format!(
                "Villagers: +{hp} HP, {melee_armour:+} melee armor, +{pierce_armour} pierce armor"
            )
        }
        BuildingLos(tiles) => format!("Buildings: +{tiles} tiles of sight"),
        AgeUp(age) => format!(
            "Advance to the {age:?} Age, unlock its buildings, units and research, and upgrade building durability and sight"
        ),
        GatherRate { resource, percent } => {
            format!("{resource:?} gathering: +{}% rate", percent - 100)
        }
        FarmFood(food) => format!("New farms: +{food} food"),
        FarmFoodAndCarry { food, carry } => {
            format!("New farms: +{food} food. Farmers: +{carry} carry capacity")
        }
        VillagerEconomy {
            capacity_permyriad,
            speed_percent,
        } => {
            let capacity = f64::from(capacity_permyriad - 10_000) / 100.0;
            format!(
                "Villagers: +{capacity}% carry capacity, +{}% movement speed",
                speed_percent - 100
            )
        }
        UnitSpeed { target, percent } => format!(
            "{}: +{}% movement speed",
            target_name(target),
            percent - 100
        ),
        UnitHp { target, hp } => format!("{}: +{hp} HP", target_name(target)),
    }
}

fn upgrade(from: impl std::fmt::Debug, to: impl std::fmt::Debug) -> String {
    format!("{} → {}", name(from), name(to))
}

fn name(value: impl std::fmt::Debug) -> String {
    let mut words = String::new();
    for ch in format!("{value:?}").chars() {
        if ch.is_ascii_uppercase() && !words.is_empty() {
            words.push(' ');
        }
        words.push(ch);
    }
    words
}

fn target_name(target: TechTarget) -> &'static str {
    match target {
        TechTarget::Infantry => "Infantry",
        TechTarget::Mounted => "Mounted units",
        TechTarget::Monk => "Monks",
        TechTarget::Trade => "Trade units",
    }
}

fn armor(tech: Tech, class: ArmorClass) -> String {
    use UnitKind::*;
    let (label, sample) = match class {
        ArmorClass::Infantry => ("Infantry", Militia),
        ArmorClass::Cavalry => ("Melee cavalry", Knight),
        ArmorClass::Archers => (
            "Foot and mounted archers, skirmishers and Hand Cannoneers",
            Archer,
        ),
        ArmorClass::Militia => ("Militia line", Militia),
        ArmorClass::MountedArchers => ("Mounted archers", CavalryArcher),
        ArmorClass::Ships => ("Ships", Galley),
    };
    let base = researched_stats(sample, true, 0);
    let improved = researched_stats(sample, true, tech.bit());
    let melee = improved.melee_armour - base.melee_armour;
    let pierce = improved.pierce_armour - base.pierce_armour;
    if melee == 0 {
        format!("{label}: +{pierce} pierce armor")
    } else {
        format!("{label}: +{melee} melee armor, +{pierce} pierce armor")
    }
}

fn attack_gain(tech: Tech, kind: UnitKind) -> u16 {
    researched_stats(kind, true, tech.bit()).attack - researched_stats(kind, true, 0).attack
}

fn rule_effect(tech: Tech, effect: RuleEffect) -> String {
    use RuleEffect::*;
    use UnitKind as U;
    let mask = tech.bit();
    match effect {
        MeleeAttack => {
            let ground = attack_gain(tech, U::Militia);
            let hulk = attack_gain(tech, U::Hulk);
            format!("Infantry and melee cavalry: +{ground} attack. Hulk line: +{hulk} attack")
        }
        Armor(class) => armor(tech, class),
        Arrows { galleys } => {
            let naval = if galleys { ", Galley line" } else { "" };
            format!(
                "Foot and mounted archers, skirmishers{naval}, towers and castles: +1 attack, range and sight. Town Centers: +1 attack and sight"
            )
        }
        ProjectileAttack => {
            let attack = attack_gain(tech, U::Archer);
            format!(
                "Foot and mounted archers, skirmishers, Galley line and ranged buildings: +{attack} attack"
            )
        }
        LeadTargets => "Non-siege ranged projectiles lead moving targets".into(),
        RemoveMinimumRange => {
            "Defensive buildings can fire at targets directly underneath them".into()
        }
        BuildingDurability(group) => durability(group, tech),
        BonusDamage(bonus) => bonus_damage(bonus, tech),
        ArcheryReload => "Foot and mounted archers and skirmishers attack 15% faster".into(),
        TowerAttack => {
            use super::super::generic_effects::tower_attack;
            let [watch, guard, keep] = [
                BuildingKind::WatchTower,
                BuildingKind::GuardTower,
                BuildingKind::Keep,
            ]
            .map(|kind| tower_attack(kind, 0, mask));
            format!("Watch Towers: +{watch} attack. Guard Towers: +{guard}. Keeps: +{keep}")
        }
        ConversionResistance => format!(
            "Enemy conversion attempts against your units succeed {}% of the time",
            conversion_chance(mask) / 10
        ),
        GarrisonHealing => "Garrisoned units heal six times as fast".into(),
        SharedConversion => {
            "When several Monks convert the same target, only the successful Monk needs to recover"
                .into()
        }
        ConversionRecovery => "Monks recover twice as fast after converting a target".into(),
        MonkRange => {
            let range = researched_stats(U::Monk, true, mask).range
                - researched_stats(U::Monk, true, 0).range;
            format!("Monks: +{range} conversion range")
        }
        Convert(ConversionTarget::Monks) => "Monks can convert enemy Monks".into(),
        Convert(ConversionTarget::SiegeAndBuildings) => {
            "Monks can convert siege units and most buildings".into()
        }
        DeathOnConversion => "Your units die when converted".into(),
        TrainingRate => format!(
            "Barracks, Archery Ranges, Stables and Castles train {}% faster",
            crate::training_speed_percent(BuildingKind::Barracks, mask) - 100
        ),
        Unimplemented => "No gameplay effect yet".into(),
        MarketCommission => format!(
            "Market commission falls to {}%",
            crate::market_commission_permyriad(3000, mask) / 100
        ),
        TributeFee => format!(
            "Tribute fee becomes {}% when tribute is available",
            crate::tribute_fee_permyriad(mask) / 100
        ),
        ConstructionRate => format!(
            "Construction is {}% faster",
            crate::build_speed_percent(mask) - 100
        ),
        FishingYield => "Fishing Ships: +10% gathering rate, +5 carry capacity".into(),
        ShipProduction => "Ships cost 20% less wood and train 50% faster".into(),
        ShipSpeed => {
            let percent =
                crate::game_world::combat::military::naval_speed(U::Galley, 100, mask) - 100;
            format!("Ships other than Fishing Ships: +{percent}% movement speed")
        }
        FireShipAttack => format!(
            "Fire Ship line: +{} attack",
            attack_gain(tech, U::FireGalley)
        ),
        DeathBlast => "Fire ships deal 30 melee damage within two tiles when sunk".into(),
    }
}

fn durability(group: BuildingGroup, tech: Tech) -> String {
    let gain = |kind| {
        crate::building_max_hp(kind, Age::Castle, tech.bit())
            - crate::building_max_hp(kind, Age::Castle, 0)
    };
    match group {
        BuildingGroup::Standard { bonus_resistance } => {
            let reduction = if bonus_resistance {
                ", reduced anti-building bonus damage"
            } else {
                ""
            };
            format!(
                "Buildings except farms, walls and gates: +10% base HP, +1 melee and pierce armor{reduction}"
            )
        }
        BuildingGroup::StoneWall => format!(
            "Stone Walls: +{} HP, +4 melee armor, +2 pierce armor",
            gain(BuildingKind::StoneWall)
        ),
        BuildingGroup::Castle => format!("Castles: +{} HP", gain(BuildingKind::Castle)),
    }
}

fn bonus_damage(bonus: AttackBonus, tech: Tech) -> String {
    match bonus {
        AttackBonus::InfantryBuildings => "Infantry: +2 attack against buildings".into(),
        AttackBonus::MountedArchersSpears => format!(
            "{}. Mounted archers: +2 attack against the Spearman line",
            armor(tech, ArmorClass::MountedArchers)
        ),
        AttackBonus::SiegeBuildings => {
            "Artillery: +1 range. Siege units: +20% bonus damage against buildings".into()
        }
        AttackBonus::FortificationsShips => {
            "Against ships, towers deal +125% attack and castles and docks deal +4 attack".into()
        }
        AttackBonus::VillagersBuildingsAndRams => {
            "Villagers: +15 attack against buildings, +3 against rams".into()
        }
    }
}
