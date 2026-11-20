use super::*;

#[test]
fn the_mangonel_line_releases_the_whole_volley_on_one_tick() {
    for (kind, count) in [
        (UnitKind::Mangonel, 6),
        (UnitKind::Onager, 8),
        (UnitKind::SiegeOnager, 10),
    ] {
        let mut world = imperial(&[]);
        world.set_fog_disabled(true);
        let shooter = world.spawn_kind(PlayerId(0), kind, at(100, 100)).unwrap();
        let target = world.spawn_villager(PlayerId(1), at(106, 100)).unwrap();
        world.issue_attack(PlayerId(0), shooter, target).unwrap();
        for _ in 0..40 {
            world.advance();
            if world.projectiles().any(|shot| shot.attacker == shooter) {
                break;
            }
        }
        let shots: Vec<_> = world.projectiles().collect();
        assert_eq!(shots.len(), count, "{kind:?}");
        assert!(shots.iter().all(|shot| shot.launched == world.tick()));
        assert_eq!(shots[0].to, at(106, 100));
        assert!(shots[1..].iter().any(|stone| stone.to != shots[0].to));
    }
}

#[test]
fn naval_artillery_uses_each_galleons_dat_blast_radius() {
    for (kind, radius) in [
        (UnitKind::CannonGalleon, 25),
        (UnitKind::EliteCannonGalleon, 30),
        (UnitKind::CatapultGalleon, 30),
    ] {
        assert_eq!(super::super::blast_hundredths(kind), radius, "{kind:?}");
    }
}
