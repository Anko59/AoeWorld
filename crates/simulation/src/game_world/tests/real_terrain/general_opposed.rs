//! Two General-led columns meet at the same wooded diagonal bridge.
use super::bridge_march::cross_fixture;
use super::*;
use crate::game_world::terrain_fixtures::{Command, build};

#[test]
fn two_generals_and_two_hundred_followers_cross_a_wooded_diagonal_bridge() {
    let mut f = build("forest-bridge");
    assert_eq!(f.world.general_status(f.primary).unwrap().army_size, 100);
    for n in 0..100 {
        let kind = match n % 10 {
            0..=3 => UnitKind::Militia,
            4..=6 => UnitKind::Archer,
            7..=8 => UnitKind::Scout,
            _ => UnitKind::BatteringRam,
        };
        let id = f
            .world
            .spawn_kind(PlayerId(0), kind, center(165 + n % 10, 175 + n / 10))
            .unwrap();
        f.ids.push(id);
    }
    let other = f
        .world
        .spawn_kind(PlayerId(0), UnitKind::General, center(161, 177))
        .unwrap();
    f.ids.push(other);
    assert_eq!(f.world.general_status(other).unwrap().army_size, 100);
    f.commands.push(Command::General(other, center(50, 50)));
    cross_fixture("forest-generals-opposed", f, 202, 30_000, |f| {
        // The single-army fixture's issue helper derives its census from all
        // ids. Here each of the two Generals has its own asserted census.
        assert_eq!(f.commands.len(), 2);
        for command in &f.commands {
            let Command::General(id, goal) = command else {
                panic!("General order")
            };
            assert_eq!(f.world.general_status(*id).unwrap().army_size, 100);
            f.world.issue_move(*id, *goal).unwrap();
        }
    });
}
