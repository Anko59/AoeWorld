use crate::{GameWorld, UnitKind};
use aoe_core::{CombatStance, EntityId, PlayerId, Seed, WorldConfig, WorldPosition};

fn check_pressure(w: &GameWorld, actor: EntityId) {
    let index = w.lookup[&actor];
    let owner = w.unit(actor).unwrap().player;
    for x in [2 * 1024 + 128, 4 * 1024, 6 * 1024 + 384] {
        for y in [2 * 1024 + 128, 4 * 1024, 6 * 1024 + 384] {
            let at = WorldPosition::new(x, y);
            let friends = 1 + w
                .units()
                .filter(|u| {
                    let dx = i64::from(u.position.x - at.x);
                    let dy = i64::from(u.position.y - at.y);
                    u.id != actor
                        && u.player == owner
                        && u.hp > 0
                        && dx * dx + dy * dy <= 1536 * 1536
                })
                .count();
            for capacity in [2, 6, 11] {
                assert_eq!(
                    w.friendly_contact_pressure_at(index, at, capacity),
                    friends >= capacity,
                    "at {at:?}, capacity {capacity}, friends {friends}"
                );
            }
        }
    }
}

#[test]
fn sparse_contact_bounds_match_bodies_across_moves_and_removals() {
    let mut w = GameWorld::new(WorldConfig::new(128, 128, Seed(1)).unwrap()).unwrap();
    let mut ids = Vec::new();
    for y in 0..5 {
        for x in 0..5 {
            let player = PlayerId(((x + y) % 2) as u16);
            let at = WorldPosition::new(2 * 1024 + 256 + x * 512, 2 * 1024 + 256 + y * 512);
            let id = w.spawn_kind(player, UnitKind::Militia, at).unwrap();
            w.issue_stance(player, &[id], CombatStance::NoAttack)
                .unwrap();
            ids.push(id);
        }
    }
    let actor = ids[0];
    check_pressure(&w, actor);
    for &id in &ids[8..13] {
        w.remove_unit(id);
    }
    check_pressure(&w, actor);
    for id in ids {
        if let Some(unit) = w.unit(id) {
            w.issue_move(
                id,
                WorldPosition::new(unit.position.x + 4 * 1024, unit.position.y),
            )
            .unwrap();
        }
    }
    for _ in 0..100 {
        w.advance();
        check_pressure(&w, actor);
    }
}
