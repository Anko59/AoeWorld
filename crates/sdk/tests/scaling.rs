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
