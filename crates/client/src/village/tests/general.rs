use super::super::steward::{panel_command, panel_values};
use aoe_core::EntityId;
use aoe_protocol::{GeneralCommand, GeneralState, GeneralStatus};
use wasm_bindgen_test::wasm_bindgen_test;

fn status() -> GeneralStatus {
    GeneralStatus {
        army_size: 42,
        state: GeneralState::Fighting,
        training: true,
        queued: 3,
        mix: [35, 30, 20, 10, 5],
        counts: [20, 10, 6, 4, 2],
        squads: [None; 10],
    }
}

#[wasm_bindgen_test]
fn the_panel_receives_state_training_mix_and_counts_in_a_fixed_layout() {
    assert_eq!(
        panel_values(EntityId(9), status()),
        [
            9.0, 42.0, 2.0, 1.0, 3.0, 35.0, 30.0, 20.0, 10.0, 5.0, 20.0, 10.0, 6.0, 4.0, 2.0, 0.0
        ]
    );
}

#[wasm_bindgen_test]
fn the_panel_receives_authoritative_squad_leaders_sizes_and_states() {
    let mut g = status();
    g.squads[0] = Some(aoe_protocol::SquadStatus {
        leader: EntityId(101),
        kind: aoe_protocol::SquadKind::Infantry,
        size: 20,
        state: GeneralState::Marching,
    });
    assert_eq!(
        &panel_values(EntityId(9), g)[15..],
        &[1.0, 101.0, 0.0, 20.0, 1.0]
    );
}

#[wasm_bindgen_test]
fn the_train_button_and_each_slider_become_general_commands() {
    let mix = status().mix;
    assert_eq!(
        panel_command(9, 1, mix),
        Some(GeneralCommand::SetTraining { enabled: true })
    );
    assert_eq!(
        panel_command(9, 0, mix),
        Some(GeneralCommand::SetTraining { enabled: false })
    );
    // Cavalry gets 70%; the remaining shares keep their relative proportions.
    assert_eq!(
        panel_command(10, 2 * 128 + 70, mix),
        Some(GeneralCommand::SetMix {
            mix: [13, 11, 70, 4, 2]
        })
    );
    assert_eq!(panel_command(10, 9 * 128, mix), None, "no such role");
    assert_eq!(panel_command(11, 0, mix), None, "no such action");
    let Some(GeneralCommand::SetMix { mix }) = panel_command(10, 50, [100, 0, 0, 0, 0]) else {
        panic!("mix command");
    };
    assert_eq!(mix, [50, 13, 13, 12, 12]);
}

#[wasm_bindgen_test]
fn strategic_armies_do_not_disclose_foreign_mix_stance_or_orders() {
    use crate::playground::game::strategic::army_record;
    use crate::strategic::ArmyKinds;
    use aoe_core::PlayerId;
    use aoe_protocol::{CombatStance, UnitKind, UnitTask};
    let mut unit = super::workers::villager(UnitTask::Moving);
    unit.kind = UnitKind::General;
    unit.general = Some(status());
    unit.stance = CombatStance::Defensive;
    let mut report = ArmyKinds::default();
    for _ in 0..42 {
        report.add(UnitKind::Archer);
    }
    let own = army_record(
        &unit,
        PlayerId(0),
        true,
        Some([30.5, 20.5]),
        Some(&report),
        0x4191f5,
    )
    .unwrap();
    assert_eq!(own[15], UnitKind::Archer as u8 as f64);
    assert_eq!(own[5], 42.0);
    assert_eq!(&own[8..10], &[30.5, 20.5]);
    assert_eq!(&own[10..15], &[20.0, 10.0, 6.0, 4.0, 2.0]);
    let foreign = army_record(
        &unit,
        PlayerId(1),
        false,
        Some([30.5, 20.5]),
        Some(&report),
        0xf54141,
    )
    .unwrap();
    assert_eq!(foreign[15], UnitKind::General as u8 as f64);
    assert_eq!(foreign[16], 0xf54141 as f64);
    assert_eq!(foreign[5], -1.0);
    assert_eq!(foreign[7], -1.0);
    assert!(foreign[8].is_nan() && foreign[9].is_nan());
    assert_eq!(&foreign[10..15], &[0.0; 5]);
    unit.hp = 0;
    assert!(army_record(&unit, PlayerId(0), false, None, None, 0).is_none());
    unit.hp = 25;
    unit.kind = UnitKind::Villager;
    assert!(army_record(&unit, PlayerId(0), false, None, None, 0).is_none());
}

#[wasm_bindgen_test]
fn site_projection_shows_foundations_and_hides_only_the_dead() {
    use crate::playground::game::strategic::site_buildings;
    use aoe_protocol::ConstructionState;
    let mut buildings: Vec<_> = (0..3).map(super::buildings::building).collect();
    buildings[0].construction = ConstructionState::UnderConstruction;
    buildings[0].progress_permille = 0;
    buildings[1].construction = ConstructionState::UnderConstruction;
    buildings[1].progress_permille = 999;
    buildings[2].hp = 0;
    let ids: Vec<_> = site_buildings(&buildings).map(|b| b.id).collect();
    assert_eq!(ids, vec![EntityId(0), EntityId(1)]);
}

#[wasm_bindgen_test]
fn site_projection_keeps_own_and_foreign_sites_at_anatolia_scale() {
    use crate::playground::game::strategic::site_buildings;
    use aoe_core::PlayerId;
    let mut buildings: Vec<_> = (0..86_700)
        .map(|id| {
            let mut b = super::buildings::building(id);
            b.player = PlayerId(1);
            b
        })
        .collect();
    // The owner's newest buildings sit last in the stream.
    buildings.push(super::buildings::building(86_700));
    buildings.push(super::buildings::building(86_701));
    let sites: Vec<_> = site_buildings(&buildings).map(|b| b.id).collect();
    assert_eq!(sites.len(), 86_702);
    assert!(sites.contains(&EntityId(86_699)));
    assert!(sites.contains(&EntityId(86_701)));
}

#[wasm_bindgen_test]
fn roads_and_bridges_do_not_displace_farms_in_the_far_view_sites() {
    use crate::playground::game::strategic::site_buildings;
    use aoe_protocol::BuildingKind;
    let mut buildings: Vec<_> = (0..2049).map(super::buildings::building).collect();
    for (index, building) in buildings[..2048].iter_mut().enumerate() {
        building.kind = if index % 2 == 0 {
            BuildingKind::Road
        } else {
            BuildingKind::Bridge
        };
    }
    buildings[2048].kind = BuildingKind::Farm;
    let sites: Vec<_> = site_buildings(&buildings).map(|b| b.kind).collect();
    assert_eq!(sites, [BuildingKind::Farm]);
}

#[wasm_bindgen_test]
fn site_record_ownership_is_explicit_even_when_player_colours_match() {
    use crate::playground::game::strategic::site_record;
    use aoe_core::PlayerId;
    let mut building = super::buildings::building(77);
    let own = site_record(&building, PlayerId(0), [65, 145, 245]);
    building.player = PlayerId(1);
    let enemy = site_record(&building, PlayerId(0), [65, 145, 245]);
    assert_eq!(own.len(), 10);
    assert_eq!(own[..7], enemy[..7]);
    assert_eq!(own[7], 1.0);
    assert_eq!(enemy[7], 0.0);
    assert_eq!([own[9], enemy[9]], [0.0, 1.0]);
}

#[wasm_bindgen_test]
fn site_record_uses_each_buildings_observed_age() {
    use crate::playground::game::strategic::site_record;
    use aoe_protocol::Age;
    let mut b = super::buildings::building(77);
    for (age, id) in [
        (Age::Dark, 109.0),
        (Age::Feudal, 71.0),
        (Age::Castle, 141.0),
        (Age::Imperial, 142.0),
    ] {
        b.age = age;
        assert_eq!(site_record(&b, aoe_core::PlayerId(0), [0; 3])[8], id);
    }
}
