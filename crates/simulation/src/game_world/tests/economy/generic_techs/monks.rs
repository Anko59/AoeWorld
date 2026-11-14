//! Monk conversion and the Monastery technologies.
use super::*;
use crate::{conversion_chance, convertible_building, convertible_unit};

/// Player 1 owns a house so it has an economy, then a unit at `x`.
fn enemy(w: &mut GameWorld, kind: UnitKind, x: i32) -> EntityId {
    if w.player_economy(PlayerId(1)).is_none() {
        w.insert_building(
            PlayerId(1),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(30, 30, 2, 2),
            true,
        )
        .unwrap();
    }
    w.spawn_kind(PlayerId(1), kind, tile(x, 90)).unwrap()
}

fn units_of(w: &GameWorld, player: u16, kind: UnitKind) -> Vec<EntityId> {
    w.units()
        .filter(|u| u.player == PlayerId(player) && u.kind == kind && u.hp > 0)
        .map(|u| u.id)
        .collect()
}

fn convert(w: &mut GameWorld, monk: EntityId, target: EntityId) {
    w.issue_attack(PlayerId(0), monk, target).unwrap();
    advance(w, 40);
}

#[test]
fn a_monk_converts_an_enemy_unit_which_keeps_its_wound() {
    let (mut w, _) = castle_player();
    let monk = w
        .spawn_kind(PlayerId(0), UnitKind::Monk, tile(90, 90))
        .unwrap();
    let villager = enemy(&mut w, UnitKind::Villager, 94);
    let at = w.lookup[&villager];
    w.units[at].state.hp -= 7;
    let hp = w.unit(villager).unwrap().hp;
    let pop = w.player_economy(PlayerId(1)).unwrap().population_used;
    let before = units_of(&w, 0, UnitKind::Villager);
    convert(&mut w, monk, villager);
    assert!(w.unit(villager).is_none(), "the old body is gone");
    let mine: Vec<_> = units_of(&w, 0, UnitKind::Villager)
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(mine.len(), 1);
    // The wound is kept (the Monk may already have healed some of it).
    let new = w.unit(mine[0]).unwrap();
    assert!(new.hp >= hp && new.hp < new.max_hp, "{} from {hp}", new.hp);
    assert!(units_of(&w, 1, UnitKind::Villager).is_empty());
    assert_eq!(
        w.player_economy(PlayerId(1)).unwrap().population_used,
        pop - 1
    );
}

#[test]
fn a_converting_monk_rests_and_illumination_halves_the_rest() {
    let (mut w, _) = castle_player();
    let monk = w
        .spawn_kind(PlayerId(0), UnitKind::Monk, tile(90, 90))
        .unwrap();
    let first = enemy(&mut w, UnitKind::Villager, 94);
    convert(&mut w, monk, first);
    let rest = w.units[w.lookup[&monk]].next_hit_tick - w.tick.0;
    assert!(
        rest > 50 * u64::from(w.config().tick_hz),
        "rested {rest} ticks"
    );
    let (mut w2, _) = castle_player();
    learn(&mut w2, Tech::Illumination);
    let monk2 = w2
        .spawn_kind(PlayerId(0), UnitKind::Monk, tile(90, 90))
        .unwrap();
    let villager = enemy(&mut w2, UnitKind::Villager, 94);
    convert(&mut w2, monk2, villager);
    let rest2 = w2.units[w2.lookup[&monk2]].next_hit_tick - w2.tick.0;
    assert!(
        rest2 < rest * 6 / 10 && rest2 > 25 * u64::from(w2.config().tick_hz),
        "{rest2} vs {rest}"
    );
}

#[test]
fn heresy_kills_the_converted_unit_of_its_owner() {
    let (mut w, _) = castle_player();
    let monk = w
        .spawn_kind(PlayerId(0), UnitKind::Monk, tile(90, 90))
        .unwrap();
    let villager = enemy(&mut w, UnitKind::Villager, 94);
    w.economy.players.get_mut(&PlayerId(1)).unwrap().researched |= Tech::Heresy.bit();
    let before = units_of(&w, 0, UnitKind::Villager).len();
    convert(&mut w, monk, villager);
    assert_eq!(
        units_of(&w, 0, UnitKind::Villager).len(),
        before,
        "nothing changed sides"
    );
    assert_eq!(w.unit(villager).map(|u| u.hp), Some(0), "the unit died");
}

#[test]
fn only_atonement_and_redemption_open_monks_and_siege_to_conversion() {
    let none = 0;
    let atonement = Tech::Atonement.bit();
    let redemption = Tech::Redemption.bit();
    assert!(convertible_unit(UnitKind::Knight, none));
    assert!(!convertible_unit(UnitKind::Monk, none));
    assert!(convertible_unit(UnitKind::Monk, atonement));
    assert!(!convertible_unit(UnitKind::Mangonel, atonement));
    assert!(convertible_unit(UnitKind::Mangonel, redemption));
    assert!(convertible_unit(UnitKind::BatteringRam, redemption));
    assert!(!convertible_unit(UnitKind::General, atonement | redemption));
    assert!(convertible_building(BuildingKind::Barracks));
    for kind in [
        BuildingKind::TownCenter,
        BuildingKind::Castle,
        BuildingKind::Monastery,
        BuildingKind::Farm,
        BuildingKind::StoneWall,
        BuildingKind::StoneGate,
        BuildingKind::PalisadeWall,
    ] {
        assert!(!convertible_building(kind), "{kind:?}");
    }
}

#[test]
fn a_monk_without_atonement_cannot_convert_an_enemy_monk() {
    let (mut w, _) = castle_player();
    let monk = w
        .spawn_kind(PlayerId(0), UnitKind::Monk, tile(90, 90))
        .unwrap();
    let other = enemy(&mut w, UnitKind::Monk, 94);
    convert(&mut w, monk, other);
    assert_eq!(w.unit(other).unwrap().player, PlayerId(1));
    learn(&mut w, Tech::Atonement);
    convert(&mut w, monk, other);
    assert!(w.unit(other).is_none());
    assert_eq!(units_of(&w, 0, UnitKind::Monk).len(), 2);
}

#[test]
fn redemption_lets_a_monk_convert_a_barracks_but_not_a_town_center() {
    let (mut w, _) = castle_player();
    learn(&mut w, Tech::Redemption);
    let monk = w
        .spawn_kind(PlayerId(0), UnitKind::Monk, tile(90, 90))
        .unwrap();
    w.insert_building(
        PlayerId(1),
        BuildingKind::Barracks,
        aoe_core::TileRect::from_xywh(94, 88, 3, 3),
        true,
    )
    .unwrap();
    let barracks = w
        .buildings()
        .find(|b| b.player == PlayerId(1) && b.kind == BuildingKind::Barracks)
        .unwrap()
        .id;
    let hp = w.building(barracks).unwrap().hp;
    convert(&mut w, monk, barracks);
    let converted: Vec<_> = w
        .buildings()
        .filter(|b| b.player == PlayerId(0) && b.kind == BuildingKind::Barracks)
        .collect();
    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0].hp, hp);
    assert!(w.building(barracks).is_none());
}

#[test]
fn theocracy_lets_only_the_converting_monk_rest() {
    let rested = |theocracy: bool| {
        let (mut w, _) = castle_player();
        if theocracy {
            learn(&mut w, Tech::Theocracy);
        }
        let a = w
            .spawn_kind(PlayerId(0), UnitKind::Monk, tile(90, 90))
            .unwrap();
        let b = w
            .spawn_kind(PlayerId(0), UnitKind::Monk, tile(90, 92))
            .unwrap();
        let villager = enemy(&mut w, UnitKind::Villager, 94);
        w.issue_attack(PlayerId(0), b, villager).unwrap();
        w.issue_attack(PlayerId(0), a, villager).unwrap();
        advance(&mut w, 40);
        let tick = w.tick.0;
        [a, b].map(|id| w.units[w.lookup[&id]].next_hit_tick > tick + 20 * 20)
    };
    let without = rested(false);
    assert_eq!(
        without,
        [true, true],
        "both monks converted the same target"
    );
    let with = rested(true);
    assert_eq!(
        with.iter().filter(|r| **r).count(),
        1,
        "one monk rests with Theocracy"
    );
}

#[test]
fn devotion_and_faith_make_units_harder_to_convert() {
    assert_eq!(conversion_chance(0), 1000);
    assert_eq!(conversion_chance(Tech::Devotion.bit()), 850);
    assert_eq!(
        conversion_chance(Tech::Devotion.bit() | Tech::Faith.bit()),
        500
    );
}

#[test]
fn block_printing_extends_monk_range_by_three_tiles() {
    let (mut w, _) = castle_player();
    let monk = w
        .spawn_kind(PlayerId(0), UnitKind::Monk, tile(90, 90))
        .unwrap();
    let range = w.unit_stats(monk).unwrap().range;
    learn(&mut w, Tech::BlockPrinting);
    assert_eq!(w.unit_stats(monk).unwrap().range, range + 3);
}
