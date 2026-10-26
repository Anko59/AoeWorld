use super::*;

#[test]
fn browser_content_roundtrips_the_current_rules_without_a_second_balance_table() {
    let content = BrowserContent::decode(&browser_content()).unwrap();
    for (view, rule) in content.buildings.iter().zip(crate::BUILDING_RULES) {
        assert_eq!(view.kind, rule.kind as u8);
        assert_eq!(view.cost, rule.cost);
        assert_eq!((view.side, view.depth), (rule.side, rule.depth));
        assert_eq!(view.age, rule.age as u8);
        assert_eq!(view.buildable, rule.buildable);
        for unit in crate::UNIT_COMBAT {
            assert_eq!(
                view.trains(unit.kind as u8),
                rule.trainable.contains(&unit.kind)
            );
        }
    }
    for (view, rule) in content.techs.iter().zip(crate::TECH_RULES) {
        assert_eq!(view.tech, rule.tech as u8);
        assert_eq!(view.cost, rule.cost);
        assert_eq!(view.age, rule.age as u8);
        assert_eq!(view.building, rule.building as u8);
        match rule.requires {
            crate::TechRequirement::Technology(t) => assert_eq!(view.requires_tech, Some(t as u8)),
            crate::TechRequirement::BuildingKinds { count, kinds } => {
                assert_eq!(usize::from(view.building_count), count);
                assert_eq!(
                    view.building_kinds,
                    kinds.iter().fold(0, |m, k| m | (1 << *k as u8))
                );
            }
            _ => assert_eq!(view.requires_tech, None),
        }
    }
    for (view, rule) in content.units.iter().zip(crate::UNIT_COMBAT) {
        assert_eq!(view.combat, rule);
        assert_eq!(view.age, rule.kind.required_age() as u8);
        assert_eq!(
            view.requires_tech,
            rule.kind.required_tech().map(|t| t as u8)
        );
        for tech in crate::TECH_RULES {
            assert_eq!(
                view.superseded(tech.tech.bit()),
                rule.kind.upgraded(tech.tech.bit()) != rule.kind
            );
        }
    }
    for researched in [
        0,
        crate::Tech::Wheelbarrow.bit(),
        crate::Tech::HandCart.bit(),
        crate::Tech::Wheelbarrow.bit() | crate::Tech::HandCart.bit(),
    ] {
        assert_eq!(
            content.villager_capacity(researched),
            crate::villager_capacity(researched)
        );
    }
}

#[test]
fn browser_content_rejects_truncated_extra_and_invalid_records() {
    let data = browser_content();
    for end in 0..data.len() {
        assert!(
            BrowserContent::decode(&data[..end]).is_err(),
            "prefix {end}"
        );
    }
    let mut extra = data.clone();
    extra.push(0);
    assert!(BrowserContent::decode(&extra).is_err());
    for index in [0, 1, 2, 3, 4] {
        let mut bad = data.clone();
        bad[index] = -1;
        assert!(BrowserContent::decode(&bad).is_err(), "header {index}");
    }
    for index in [10, 11] {
        let mut bad = data.clone();
        bad[index] = 1_000_000;
        assert!(
            BrowserContent::decode(&bad).is_err(),
            "unbounded footprint {index}"
        );
    }
}
