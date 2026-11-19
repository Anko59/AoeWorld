use aoe_protocol::GameplayClientMessage as M;
use aoe_sdk::{EntityId, Order};

fn id(n: u32) -> EntityId {
    EntityId(n)
}

#[test]
fn technology_catalog_covers_the_current_tree_and_exposes_dependencies() {
    use aoe_sdk::{Age, BuildingKind, Tech, rules};
    let all = rules::technologies();
    assert_eq!(all.len(), Tech::ALL.len());
    for tech in Tech::ALL {
        assert_eq!(all.iter().filter(|row| row.tech == tech).count(), 1);
    }
    let champion = all.iter().find(|row| row.tech == Tech::Champion).unwrap();
    assert_eq!(champion.building, BuildingKind::Barracks);
    assert_eq!(champion.age, Age::Imperial);
    assert_eq!(champion.prerequisite, Some(Tech::TwoHandedSwordsman));
    let imperial = all
        .iter()
        .find(|row| row.tech == Tech::ImperialAge)
        .unwrap();
    assert_eq!(imperial.building_count, 2);
    assert!(imperial.building_kinds.contains(&BuildingKind::University));
    assert!(imperial.building_kinds.contains(&BuildingKind::Castle));
}

#[test]
fn management_settings_use_the_same_public_orders_as_the_browser() {
    assert_eq!(
        Order::StewardSettings {
            unit: id(17),
            weights: [45, 30, 15, 10],
            cap: 100,
            training: true,
        }
        .message(7),
        Some(M::StewardSettingsOrder {
            sequence: 7,
            steward_id: id(17),
            weights: [45, 30, 15, 10],
            cap: 100,
            training: true,
        })
    );
    assert_eq!(
        Order::VassalDevelopment {
            unit: id(18),
            enabled: true
        }
        .message(8),
        Some(M::VassalDevelopmentOrder {
            sequence: 8,
            vassal_id: id(18),
            enabled: true
        })
    );
}

#[test]
fn market_quotes_include_the_same_commission_as_the_server() {
    assert_eq!(aoe_sdk::rules::market_quote(100, true), 130);
    assert_eq!(aoe_sdk::rules::market_quote(100, false), 70);
}

#[test]
fn training_catalog_follows_live_age_and_technology_rules() {
    use aoe_sdk::{Age, BuildingKind, Tech, UnitKind, rules};

    let workshop = BuildingKind::SiegeWorkshop;
    let base = rules::training_options(workshop, Age::Castle, &[]);
    assert!(base.contains(&UnitKind::Scorpion));
    assert!(!base.contains(&UnitKind::BombardCannon));
    assert!(!base.contains(&UnitKind::SiegeOnager));
    let upgraded = rules::training_options(
        workshop,
        Age::Imperial,
        &[Tech::Chemistry, Tech::Onager, Tech::SiegeOnager],
    );
    assert!(upgraded.contains(&UnitKind::BombardCannon));
    assert!(upgraded.contains(&UnitKind::SiegeOnager));
    assert!(!upgraded.contains(&UnitKind::Mangonel));
    let ships = rules::training_options(
        BuildingKind::Dock,
        Age::Imperial,
        &[Tech::WarGalley, Tech::HeavyWarships],
    );
    assert!(ships.contains(&UnitKind::Carrack));
    assert!(ships.contains(&UnitKind::FastFireShip));
    assert!(ships.contains(&UnitKind::Galleon));
    assert!(!ships.contains(&UnitKind::Galley));
    assert_eq!(rules::unit_profile(UnitKind::Carrack).line, UnitKind::Hulk);
    assert_eq!(
        rules::unit_profile(UnitKind::FastFireShip).line,
        UnitKind::FireGalley
    );
    assert_eq!(
        rules::unit_profile(UnitKind::TrebuchetUnpacked).line,
        UnitKind::Trebuchet
    );

    let technologies = rules::technologies();
    let chemistry = technologies
        .iter()
        .find(|r| r.tech == Tech::Chemistry)
        .unwrap();
    assert!(chemistry.unlocks.contains(&UnitKind::HandCannoneer));
    assert!(chemistry.unlocks.contains(&UnitKind::BombardCannon));
    let husbandry = technologies
        .iter()
        .find(|r| r.tech == Tech::Husbandry)
        .unwrap();
    assert!(husbandry.improves.contains(&UnitKind::CamelRider));
    assert!(!husbandry.improves.contains(&UnitKind::Spearman));
    let warships = technologies
        .iter()
        .find(|r| r.tech == Tech::HeavyWarships)
        .unwrap();
    assert!(warships.upgrades.contains(&UnitKind::Galley));
    assert!(warships.upgrades.contains(&UnitKind::FireGalley));
    assert!(warships.upgrades.contains(&UnitKind::Hulk));
}

#[test]
fn planning_bonuses_use_the_game_combat_classes() {
    use aoe_sdk::{BuildingKind, UnitKind, rules};
    assert!(
        rules::unit_counter_bonus(UnitKind::CamelRider, UnitKind::Knight)
            > rules::unit_counter_bonus(UnitKind::CamelRider, UnitKind::Spearman)
    );
    assert!(
        rules::unit_building_bonus(UnitKind::Trebuchet, BuildingKind::Castle)
            > rules::unit_building_bonus(UnitKind::Scorpion, BuildingKind::Castle)
    );
    assert!(rules::unit_profile(UnitKind::Trebuchet).armed);
    assert!(rules::unit_profile(UnitKind::Hulk).naval);
    assert!(!rules::unit_profile(UnitKind::FishingShip).armed);
}
