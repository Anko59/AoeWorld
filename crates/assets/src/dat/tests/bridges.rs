use super::*;

#[test]
fn real_trial_bridge_scenario_ids_resolve_the_original_piece_graphics() {
    let Ok(path) = std::env::var("AOE_TRIAL_DAT") else {
        return;
    };
    let file = decode(&std::fs::read(path).unwrap()).unwrap();
    for (unit, graphic, slp, footprint) in [
        (605, 3331, 3688, [2.0, 3.0]),
        (606, 3333, 3690, [2.0, 3.0]),
        (607, 3335, 3692, [2.0, 3.0]),
        (608, 3337, 3694, [3.0, 2.0]),
        (609, 3339, 3696, [3.0, 2.0]),
        (610, 3341, 3698, [3.0, 2.0]),
    ] {
        let recipe = file.building_composition(unit).unwrap();
        assert_eq!(recipe.footprint, footprint);
        assert!(
            recipe
                .parts
                .iter()
                .any(|p| p.graphic_id == graphic && p.slp_id == slp)
        );
    }
}
