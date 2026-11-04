use super::*;
use crate::GameplayService;

#[test]
fn census_scans_once_for_a_thousand_players_and_only_builds_connected_reports() {
    let mut world = GameplayService::census_benchmark_world().unwrap();
    assert_eq!(world.players().count(), 1000);
    assert!(viewer_states(&world, &[], &BTreeMap::new()).is_empty());
    assert_eq!(world.unit_census_rebuilds(), 0);
    let views = super::super::benchmark::views(1000);
    for connected in [1, 1000, 1, 0, 1000] {
        let states = viewer_states(&world, &views[..connected], &BTreeMap::new());
        assert_eq!(states.len(), connected);
        for (id, state) in states {
            assert_eq!(state.players.len(), 1);
            let own = &state.players[&id];
            assert!(own.held_tiles > 0);
            assert!(own.held_share_ppm > 0);
            for span in &own.holdings {
                assert!((span.x0..span.x1).all(|x| {
                    world.visibility(id, aoe_core::TileCoord::new(x, span.y))
                        != aoe_simulation::TileVisibility::Unexplored
                }));
            }
            assert_eq!(own.unit_counts.iter().sum::<u32>(), 100);
            assert_eq!(own.unit_counts[aoe_protocol::UnitKind::Scout as usize], 100);
        }
        assert_eq!(world.unit_census_rebuilds(), 1);
        assert_eq!(world.land_control_rebuilds(), 1);
    }
    let position = world.units().next().unwrap().position;
    world
        .spawn_kind(PlayerId(999), aoe_simulation::UnitKind::Militia, position)
        .unwrap();
    let states = viewer_states(&world, &views[999..], &BTreeMap::new());
    let own = &states[&PlayerId(999)].players[&PlayerId(999)];
    assert_eq!(own.unit_counts[aoe_protocol::UnitKind::Scout as usize], 100);
    assert_eq!(own.unit_counts[aoe_protocol::UnitKind::Militia as usize], 1);
    assert_eq!(own.unit_counts.iter().sum::<u32>(), 101);
    assert_eq!(world.unit_census_rebuilds(), 1);
}
