//! Unit effects: speed, hit points, armour, fire rate and attack bonuses.
use super::*;

fn speed(w: &GameWorld, kind: UnitKind) -> u64 {
    let (n, d) = w.movement_speed(kind, PlayerId(0));
    n * 10_000 / d
}

/// Speeds are whole hundredths of a tile per second, so a percentage rounds.
fn assert_faster(after: u64, before: u64, percent: u64) {
    let expected = before * percent / 100;
    assert!(
        after.abs_diff(expected) <= before / 100,
        "{after} vs {expected}"
    );
    assert!(after > before);
}

#[test]
fn squires_speed_infantry_by_ten_percent_and_leave_cavalry_alone() {
    let (mut w, _) = castle_player();
    let (infantry, cavalry) = (speed(&w, UnitKind::Spearman), speed(&w, UnitKind::Knight));
    learn(&mut w, Tech::Squires);
    assert_faster(speed(&w, UnitKind::Spearman), infantry, 110);
    assert_eq!(speed(&w, UnitKind::Knight), cavalry);
}

#[test]
fn husbandry_speeds_cavalry_by_ten_percent_including_scouts() {
    let (mut w, _) = castle_player();
    let (knight, scout, spear) = (
        speed(&w, UnitKind::Knight),
        speed(&w, UnitKind::Scout),
        speed(&w, UnitKind::Spearman),
    );
    learn(&mut w, Tech::Husbandry);
    assert_faster(speed(&w, UnitKind::Knight), knight, 110);
    assert_faster(speed(&w, UnitKind::Scout), scout, 110);
    assert_eq!(speed(&w, UnitKind::Spearman), spear);
}

#[test]
fn bloodlines_add_twenty_hp_to_living_and_new_cavalry_only() {
    let (mut w, _) = castle_player();
    let knight = w
        .spawn_kind(PlayerId(0), UnitKind::Knight, tile(90, 90))
        .unwrap();
    let spear = w
        .spawn_kind(PlayerId(0), UnitKind::Spearman, tile(92, 90))
        .unwrap();
    let base = w.unit(knight).unwrap().max_hp;
    let spear_hp = w.unit(spear).unwrap().max_hp;
    let wounded = w.lookup[&knight];
    w.units[wounded].state.hp -= 10;
    learn(&mut w, Tech::Bloodlines);
    let k = w.unit(knight).unwrap();
    assert_eq!((k.max_hp, k.hp), (base + 20, base + 10), "damage is kept");
    assert_eq!(w.unit(spear).unwrap().max_hp, spear_hp);
    let fresh = w
        .spawn_kind(PlayerId(0), UnitKind::Cavalier, tile(94, 90))
        .unwrap();
    assert_eq!(
        w.unit(fresh).unwrap().max_hp,
        crate::unit_combat(UnitKind::Cavalier).hp + 20
    );
}

#[test]
fn sanctity_adds_fifteen_hp_to_monks_and_fervor_speeds_them() {
    let (mut w, _) = castle_player();
    let monk = w
        .spawn_kind(PlayerId(0), UnitKind::Monk, tile(90, 90))
        .unwrap();
    let base = w.unit(monk).unwrap().max_hp;
    let walk = speed(&w, UnitKind::Monk);
    learn(&mut w, Tech::Sanctity);
    learn(&mut w, Tech::Fervor);
    assert_eq!(w.unit(monk).unwrap().max_hp, base + 15);
    assert_faster(speed(&w, UnitKind::Monk), walk, 115);
}

#[test]
fn gambesons_add_pierce_armour_to_the_militia_line_only() {
    let (mut w, _) = castle_player();
    let militia = w
        .spawn_kind(PlayerId(0), UnitKind::LongSwordsman, tile(90, 90))
        .unwrap();
    let spear = w
        .spawn_kind(PlayerId(0), UnitKind::Spearman, tile(92, 90))
        .unwrap();
    let (m0, s0) = (
        w.unit_stats(militia).unwrap().pierce_armour,
        w.unit_stats(spear).unwrap().pierce_armour,
    );
    learn(&mut w, Tech::Gambesons);
    assert_eq!(w.unit_stats(militia).unwrap().pierce_armour, m0 + 1);
    assert_eq!(w.unit_stats(spear).unwrap().pierce_armour, s0);
}

#[test]
fn thumb_ring_shortens_the_archer_reload_by_fifteen_percent() {
    let (mut w, _) = castle_player();
    let archer = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, tile(90, 90))
        .unwrap();
    let knight = w
        .spawn_kind(PlayerId(0), UnitKind::Knight, tile(92, 90))
        .unwrap();
    let ticks = |w: &GameWorld, id| w.attack_period_ticks(&w.unit(id).unwrap());
    let (a0, k0) = (ticks(&w, archer), ticks(&w, knight));
    learn(&mut w, Tech::ThumbRing);
    assert_eq!(ticks(&w, archer), (a0 * 100 / 115).max(1));
    assert_eq!(ticks(&w, knight), k0);
    assert!(w.unit_stats(archer).unwrap().attack_period_centiseconds < 200);
}

#[test]
fn siege_engineers_add_range_to_artillery_and_a_fifth_to_its_building_bonus() {
    let (mut w, _) = castle_player();
    let mangonel = w
        .spawn_kind(PlayerId(0), UnitKind::Mangonel, tile(90, 90))
        .unwrap();
    let range = w.unit_stats(mangonel).unwrap().range;
    learn(&mut w, Tech::SiegeEngineers);
    assert_eq!(w.unit_stats(mangonel).unwrap().range, range + 1);
    let enemy = w
        .insert_building(
            PlayerId(1),
            BuildingKind::Barracks,
            aoe_core::TileRect::from_xywh(100, 100, 3, 3),
            true,
        )
        .unwrap();
    assert_eq!(
        w.tech_attack_bonus(UnitKind::Mangonel, PlayerId(0), enemy, 100),
        20
    );
    assert_eq!(
        w.tech_attack_bonus(UnitKind::Archer, PlayerId(0), enemy, 100),
        0
    );
}

#[test]
fn arson_and_sappers_add_attack_against_buildings_and_rams() {
    let (mut w, _) = castle_player();
    let enemy = w
        .insert_building(
            PlayerId(1),
            BuildingKind::Barracks,
            aoe_core::TileRect::from_xywh(100, 100, 3, 3),
            true,
        )
        .unwrap();
    let ram = w
        .spawn_kind(PlayerId(1), UnitKind::BatteringRam, tile(110, 110))
        .unwrap();
    let bonus = |w: &GameWorld, kind, target| w.tech_attack_bonus(kind, PlayerId(0), target, 0);
    assert_eq!(bonus(&w, UnitKind::Militia, enemy), 0);
    learn(&mut w, Tech::Arson);
    assert_eq!(bonus(&w, UnitKind::Militia, enemy), 2);
    assert_eq!(bonus(&w, UnitKind::Archer, enemy), 0);
    learn(&mut w, Tech::Sappers);
    assert_eq!(bonus(&w, UnitKind::Villager, enemy), 15);
    assert_eq!(bonus(&w, UnitKind::Villager, ram), 3);
}

#[test]
fn caravan_speeds_trade_units_and_leaves_land_units_alone() {
    let (mut w, _) = castle_player();
    let spear = speed(&w, UnitKind::Spearman);
    assert_eq!(crate::trade_speed_percent(researched(&w)), 100);
    learn(&mut w, Tech::Caravan);
    assert_eq!(crate::trade_speed_percent(researched(&w)), 120);
    assert_eq!(speed(&w, UnitKind::Spearman), spear);
}
