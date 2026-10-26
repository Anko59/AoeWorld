use super::*;

#[test]
fn monks_never_heal_naval_units_or_siege() {
    for kind in [
        UnitKind::FishingShip,
        UnitKind::Galley,
        UnitKind::WarGalley,
        UnitKind::FireShip,
        UnitKind::DemolitionShip,
        UnitKind::TransportShip,
        UnitKind::Mangonel,
        UnitKind::BatteringRam,
    ] {
        let mut world = coast();
        let (land, water) = bank(&world);
        world
            .spawn_kind(PlayerId(0), UnitKind::Monk, at(land))
            .unwrap();
        let target = world
            .spawn_kind(
                PlayerId(0),
                kind,
                at(if kind.is_naval() { water } else { land }),
            )
            .unwrap();
        let hp = world.unit(target).unwrap().max_hp - 10;
        world.units[world.lookup[&target]].state.hp = hp;
        advance(&mut world, 40);
        assert_eq!(world.unit(target).unwrap().hp, hp, "{kind:?}");
    }
}

#[test]
fn embarked_monk_stops_healing_at_the_bank_and_resumes_after_unloading() {
    fn run() -> [u8; 32] {
        let mut world = coast();
        let (land, water) = bank(&world);
        let transport = ship(&mut world, UnitKind::TransportShip, water, 0);
        let monk = world
            .spawn_kind(PlayerId(0), UnitKind::Monk, at(land))
            .unwrap();
        let knight = world
            .spawn_kind(PlayerId(0), UnitKind::Knight, at(land))
            .unwrap();
        world.units[world.lookup[&knight]].state.hp = 50;
        advance(&mut world, 8);
        assert_eq!(world.unit(knight).unwrap().hp, 51);
        world.issue_load(PlayerId(0), &[monk], transport).unwrap();
        let destination = TileCoord::new(water.x + 9, water.y);
        world.issue_move(transport, at(destination)).unwrap();
        advance(&mut world, 300);
        assert_eq!(world.unit(transport).unwrap().position, at(destination));
        assert_eq!(
            world.unit(knight).unwrap().hp,
            51,
            "cargo cannot heal at its retained boarding position"
        );
        assert_eq!(world.transport_cargo(transport), &[monk]);
        world.issue_move(transport, at(water)).unwrap();
        advance(&mut world, 300);
        assert_eq!(world.unit(knight).unwrap().hp, 51);
        world.issue_unload(PlayerId(0), transport, land).unwrap();
        advance(&mut world, 8);
        assert_eq!(world.unit(knight).unwrap().hp, 52);
        world.canonical_hash()
    }
    assert_eq!(run(), run());
}
