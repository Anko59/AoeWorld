//! DE class rows; sources and ruleset are in docs/gameplay/hidden-bonuses.md.
use super::*;
use crate::{Age, BuildingKind, Tech, UnitKind};

#[test]
fn hidden_siege_and_naval_building_components_see_castle_and_masonry_armour() {
    for (attacker, house, castle) in [
        (UnitKind::Mangonel, 35, 27),
        (UnitKind::BatteringRam, 150, 142),
        (UnitKind::Galley, 6, 0),
        (UnitKind::WarGalley, 7, 0),
        (UnitKind::FireShip, 5, 0), // +2 building and 1 - (-2) melee.
        (UnitKind::DemolitionShip, 220, 212),
    ] {
        let mut w = world();
        let h = w
            .insert_building(
                PlayerId(1),
                BuildingKind::House,
                TileRect::from_xywh(100, 100, 2, 2),
                true,
            )
            .unwrap();
        let c = w
            .insert_building(
                PlayerId(1),
                BuildingKind::Castle,
                TileRect::from_xywh(110, 100, 4, 4),
                true,
            )
            .unwrap();
        assert_eq!(w.unit_damage_bonus(attacker, h), house, "{attacker:?}");
        assert_eq!(w.unit_damage_bonus(attacker, c), castle, "{attacker:?}");
    }
}

#[test]
fn hidden_projectile_building_bonus_survives_the_shooters_removal() {
    let mut w = world();
    let target = w
        .insert_building(
            PlayerId(1),
            BuildingKind::House,
            TileRect::from_xywh(105, 100, 2, 2),
            true,
        )
        .unwrap();
    let attacker = w
        .spawn_kind(PlayerId(0), UnitKind::Mangonel, at(100, 100))
        .unwrap();
    let hp = w.building(target).unwrap().hp;
    w.issue_attack(PlayerId(0), attacker, target).unwrap();
    w.advance();
    assert_eq!(w.projectiles().count(), 1);
    w.remove_unit(attacker);
    advance(&mut w, 40);
    assert_eq!(hp - w.building(target).unwrap().hp, 77); // 40 - (-2) + 35 All Buildings.
}

#[test]
fn hidden_unit_bonus_rows_are_independent_of_base_armour() {
    use UnitKind::*;
    for (attacker, victim, bonus) in [
        (Spearman, Knight, 15),
        (Pikeman, Knight, 22),
        (Spearman, Galley, 9),
        (Pikeman, Galley, 16),
        (Spearman, FishingShip, 9),
        (Pikeman, FishingShip, 16),
        (Skirmisher, Archer, 3),
        (Skirmisher, Spearman, 3),
        (EliteSkirmisher, Crossbowman, 4),
        (EliteSkirmisher, Pikeman, 4),
        (Archer, Spearman, 3),
        (Crossbowman, Pikeman, 3),
        (Scout, Monk, 6),
        (LightCavalry, Monk, 10),
        (BatteringRam, Mangonel, 40),
        (BatteringRam, BatteringRam, 40),
        (Mangonel, Mangonel, 12),
        (Mangonel, BatteringRam, 12),
        (Galley, Galley, 8),
        (Galley, FireShip, 2),
        (Galley, DemolitionShip, 5),
        (Galley, FishingShip, 8),
        (Galley, BatteringRam, 3),
        (WarGalley, Galley, 9),
        (WarGalley, FireShip, 3),
        (WarGalley, BatteringRam, 4),
        (Villager, Knight, 0),
        (Militia, Villager, 0),
        (Knight, Spearman, 0),
        (Monk, Scout, 0),
        (FishingShip, Galley, 0),
        (TransportShip, Galley, 0),
        (DemolitionShip, Galley, 0),
        (Vassal, Knight, 0),
    ] {
        assert_eq!(
            crate::game_world::combat::military::class_bonus(attacker, victim),
            bonus,
            "{attacker:?} -> {victim:?}"
        );
    }
}

#[test]
fn hidden_building_bonus_rows_apply_once_and_masonry_only_reduces_building_class() {
    use UnitKind::*;
    for (attacker, building, expected, masonry_expected) in [
        (Villager, BuildingKind::House, 8, 4),
        (Villager, BuildingKind::WatchTower, 11, 7),
        (Villager, BuildingKind::Castle, 1, 1),
        (Militia, BuildingKind::House, 6, 5),
        (ManAtArms, BuildingKind::House, 10, 9),
        (LongSwordsman, BuildingKind::House, 14, 13),
        (Spearman, BuildingKind::House, 6, 5),
        (Pikeman, BuildingKind::House, 7, 6),
        (BatteringRam, BuildingKind::House, 154, 150),
        (BatteringRam, BuildingKind::Castle, 142, 139),
    ] {
        for (masonry, expected) in [(false, expected), (true, masonry_expected)] {
            let mut w = world();
            super::military::feudal(&mut w);
            let mut defender = w.player_economy(PlayerId(0)).unwrap();
            defender.player = PlayerId(1);
            defender.age = Age::Dark;
            defender.researched = if masonry { Tech::Masonry.bit() } else { 0 };
            w.economy.players.insert(PlayerId(1), defender);
            let b = w
                .insert_building(
                    PlayerId(1),
                    building,
                    TileRect::from_xywh(100, 100, 4, 4),
                    true,
                )
                .unwrap();
            let a = w.spawn_kind(PlayerId(0), attacker, at(99, 100)).unwrap();
            let hp = w.building(b).unwrap().hp;
            w.issue_attack(PlayerId(0), a, b).unwrap();
            w.advance();
            assert_eq!(
                hp - w.building(b).unwrap().hp,
                expected,
                "{attacker:?} -> {building:?}, masonry={masonry}"
            );
        }
    }
}

#[test]
fn pikeman_upgrade_changes_cavalry_bonus_and_retains_blacksmith_damage() {
    let mut w = world();
    super::military::feudal(&mut w);
    let a = w
        .spawn_kind(PlayerId(0), UnitKind::Spearman, at(100, 100))
        .unwrap();
    let b = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(101, 100))
        .unwrap();
    w.complete_research(PlayerId(0), Tech::Pikeman, &mut Vec::new());
    w.complete_research(PlayerId(0), Tech::Forging, &mut Vec::new());
    w.issue_attack(PlayerId(0), a, b).unwrap();
    w.advance();
    assert_eq!(w.unit(a).unwrap().kind, UnitKind::Pikeman);
    assert_eq!(w.unit(b).unwrap().hp, 75); // (4 + 1 - 2) melee + 22 cavalry.
}

#[test]
fn hidden_standard_building_class_covers_the_entire_building_roster() {
    use BuildingKind::*;
    for (kind, expected) in [
        (TownCenter, 5),
        (House, 10),
        (LumberCamp, 8),
        (MiningCamp, 8),
        (Mill, 8),
        (Farm, 8),
        (Barracks, 8),
        (PalisadeWall, 6),
        (Outpost, 8),
        (Dock, 8),
        (ArcheryRange, 7),
        (Stable, 7),
        (Blacksmith, 7),
        (Market, 7),
        (WatchTower, 7),
        (PalisadeGate, 8),
        (Castle, 2),
        (Monastery, 6),
        (University, 6),
        (SiegeWorkshop, 7),
        (Bridge, 6),
    ] {
        let mut w = world();
        let b = w
            .insert_building(PlayerId(1), kind, TileRect::from_xywh(100, 100, 4, 4), true)
            .unwrap();
        let a = w
            .spawn_kind(PlayerId(0), UnitKind::ManAtArms, at(99, 100))
            .unwrap();
        let hp = w.building(b).unwrap().hp;
        w.issue_attack(PlayerId(0), a, b).unwrap();
        w.advance();
        assert_eq!(hp - w.building(b).unwrap().hp, expected, "{kind:?}");
    }
}
