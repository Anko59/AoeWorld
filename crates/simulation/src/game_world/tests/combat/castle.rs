//! Integer Castle mechanics; values from the local licensed DE VER 8.9 DAT.
use super::*;
use crate::{Age, BuildingKind, Tech, UnitKind};

fn castle() -> GameWorld {
    let mut w = world();
    super::military::feudal(&mut w);
    w.complete_research(PlayerId(0), Tech::CastleAge, &mut Vec::new());
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().age, Age::Castle);
    w
}

#[test]
fn castle_units_have_dat_stats_and_costs() {
    for (kind, hp, attack, melee, pierce, food, wood, gold, seconds) in [
        (UnitKind::Knight, 100, 10, 2, 2, 60, 0, 75, 30),
        (UnitKind::LightCavalry, 60, 7, 0, 2, 80, 0, 0, 30),
        (UnitKind::LongSwordsman, 60, 9, 1, 1, 50, 0, 20, 21),
        (UnitKind::Pikeman, 55, 4, 0, 0, 35, 25, 0, 22),
        (UnitKind::Crossbowman, 35, 5, 0, 0, 0, 25, 45, 27),
        (UnitKind::EliteSkirmisher, 35, 3, 0, 4, 25, 35, 0, 22),
        (UnitKind::Mangonel, 50, 40, 0, 6, 0, 160, 135, 46),
        (UnitKind::BatteringRam, 175, 2, -3, 180, 0, 160, 75, 36),
        (UnitKind::Monk, 30, 0, 0, 0, 0, 0, 100, 51),
    ] {
        let s = crate::unit_combat(kind);
        let t = s.training.unwrap();
        assert_eq!(
            (s.hp, s.attack, s.melee_armour, s.pierce_armour),
            (hp, attack, melee, pierce)
        );
        assert_eq!(
            (t.cost.food, t.cost.wood, t.cost.gold, t.seconds),
            (food, wood, gold, seconds)
        );
    }
}

#[test]
fn ram_building_bonus_is_distinct_from_base_melee_damage() {
    let mut w = castle();
    let b = w
        .insert_building(
            PlayerId(1),
            BuildingKind::House,
            TileRect::from_xywh(100, 100, 2, 2),
            true,
        )
        .unwrap();
    let ram = w
        .spawn_kind(PlayerId(0), UnitKind::BatteringRam, at(99, 100))
        .unwrap();
    let hp = w.building(b).unwrap().hp;
    w.issue_attack(PlayerId(0), ram, b).unwrap();
    w.advance();
    assert_eq!(hp - w.building(b).unwrap().hp, 154);
}

#[test]
fn mangonel_splash_is_local_includes_friendly_fire_and_replays() {
    fn run() -> ([u16; 3], [u8; 32]) {
        let mut w = castle();
        let m = w
            .spawn_kind(PlayerId(0), UnitKind::Mangonel, at(100, 100))
            .unwrap();
        let target = w
            .spawn_kind(PlayerId(1), UnitKind::Knight, at(105, 100))
            .unwrap();
        let nearby = w
            .spawn_kind(PlayerId(0), UnitKind::Knight, at(105, 101))
            .unwrap();
        let far = w
            .spawn_kind(PlayerId(1), UnitKind::Knight, at(110, 100))
            .unwrap();
        w.issue_attack(PlayerId(0), m, target).unwrap();
        advance(&mut w, 30);
        (
            [target, nearby, far].map(|id| w.unit(id).unwrap().hp),
            w.canonical_hash(),
        )
    }
    let (hp, hash) = run();
    assert!(hp[0] < 100 && hp[1] < 100);
    assert_eq!(hp[2], 100);
    assert_eq!((hp, hash), run());
}

#[test]
fn monk_heals_injured_owned_units_without_overhealing_or_healing_enemies() {
    let mut w = castle();
    let monk = w
        .spawn_kind(PlayerId(0), UnitKind::Monk, at(100, 100))
        .unwrap();
    let friend = w
        .spawn_kind(PlayerId(0), UnitKind::Knight, at(102, 100))
        .unwrap();
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(102, 101))
        .unwrap();
    for id in [friend, enemy] {
        let i = w.lookup[&id];
        w.units[i].state.hp = 90;
        // This fixture measures healing, with both armies holding fire.
        let player = w.unit(id).unwrap().player;
        w.issue_stance(player, &[id], aoe_core::CombatStance::NoAttack)
            .unwrap();
    }
    let ticks = 5 * w.config().tick_hz;
    advance(&mut w, ticks);
    assert_eq!(w.unit(friend).unwrap().hp, 100);
    assert_eq!(w.unit(enemy).unwrap().hp, 90);
    assert_eq!(w.unit(monk).unwrap().hp, 30);
}

#[test]
fn idle_monks_scan_a_healthy_crowded_bucket_only_once_per_healing_tick() {
    let mut w = castle();
    for _ in 0..10_000 {
        w.spawn_kind(PlayerId(0), UnitKind::Knight, at(110, 110))
            .unwrap();
    }
    for _ in 0..100 {
        w.spawn_kind(PlayerId(0), UnitKind::Monk, at(111, 110))
            .unwrap();
    }
    w.tick = aoe_core::Tick(8);
    let mut changed = Vec::new();
    w.advance_combat(&mut changed);
    assert!(changed.is_empty());
    assert_eq!(w.combat_stats().healing_scanned_units, 10_100);
    assert_eq!(w.combat_stats().healing_candidates, 0);
}

#[test]
fn monks_choose_nearest_then_id_across_buckets_and_recheck_full_health() {
    fn run() -> [u8; 32] {
        let mut w = castle();
        for _ in 0..2 {
            w.spawn_kind(PlayerId(0), UnitKind::Monk, at(32, 100))
                .unwrap();
        }
        let first = w
            .spawn_kind(PlayerId(0), UnitKind::Knight, at(31, 100))
            .unwrap();
        let second = w
            .spawn_kind(PlayerId(0), UnitKind::Knight, at(33, 100))
            .unwrap();
        for id in [first, second] {
            w.units[w.lookup[&id]].state.hp = 99;
        }
        // Reverse the dense storage order: equal distances still resolve by id.
        let a = w.lookup[&first];
        let b = w.lookup[&second];
        w.units.swap(a, b);
        w.lookup.insert(first, b);
        w.lookup.insert(second, a);
        w.collisions.reindex(first, b);
        w.collisions.reindex(second, a);
        w.tick = aoe_core::Tick(8);
        let mut changed = Vec::new();
        w.advance_combat(&mut changed);
        assert_eq!(changed, [first, second]);
        assert_eq!(w.unit(first).unwrap().hp, 100);
        assert_eq!(w.unit(second).unwrap().hp, 100);
        w.canonical_hash()
    }
    assert_eq!(run(), run());
}

#[test]
fn castle_targets_all_four_footprint_range_edges_and_replays() {
    fn run(x: i32, y: i32) -> (u16, [u8; 32]) {
        let mut w = castle();
        w.set_fog_disabled(true);
        w.insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(100, 100, 4, 4),
            true,
        )
        .unwrap();
        let enemy = w
            .spawn_kind(PlayerId(1), UnitKind::Knight, at(x, y))
            .unwrap();
        advance(&mut w, 60);
        (w.unit(enemy).unwrap().hp, w.canonical_hash())
    }
    for (x, y) in [(92, 102), (111, 102), (102, 92), (102, 111)] {
        let result = run(x, y);
        assert!(
            result.0 < 100,
            "Castle must hit the enemy at ({x}.5, {y}.5)"
        );
        assert_eq!(result, run(x, y));
    }
    for (x, y) in [(91, 102), (112, 102), (102, 91), (102, 112)] {
        assert_eq!(run(x, y).0, 100, "outside footprint range at ({x}, {y})");
    }
}

#[test]
fn castle_targets_exact_footprint_range_boundary_but_not_one_subunit_beyond() {
    use aoe_core::FIXED_SUBUNITS_PER_TILE as TILE;
    for (x, y, dx, dy) in [
        (92 * TILE, 102 * TILE + TILE / 2, -1, 0),
        (112 * TILE, 102 * TILE + TILE / 2, 1, 0),
        (102 * TILE + TILE / 2, 92 * TILE, 0, -1),
        (102 * TILE + TILE / 2, 112 * TILE, 0, 1),
    ] {
        for outside in [false, true] {
            let mut w = castle();
            w.set_fog_disabled(true);
            w.insert_building(
                PlayerId(0),
                BuildingKind::Castle,
                TileRect::from_xywh(100, 100, 4, 4),
                true,
            )
            .unwrap();
            let step = i32::from(outside);
            let enemy = w
                .spawn_kind(
                    PlayerId(1),
                    UnitKind::Knight,
                    WorldPosition::new(x + dx * step, y + dy * step),
                )
                .unwrap();
            advance(&mut w, 60);
            assert_eq!(
                w.unit(enemy).unwrap().hp < 100,
                !outside,
                "({x}, {y}) outside={outside}"
            );
        }
    }
}

#[test]
fn castle_fires_and_murder_holes_removes_minimum_range() {
    let mut w = castle();
    let c = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(100, 100, 4, 4),
            true,
        )
        .unwrap();
    assert_eq!(
        (
            w.building(c).unwrap().hp,
            BuildingKind::Castle.rules().cost.stone
        ),
        (4800, 650)
    );
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(106, 100))
        .unwrap();
    // This range/armour fixture holds fire; pursuit is covered separately.
    w.issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::NoAttack)
        .unwrap();
    let ticks = 2 * w.config().tick_hz;
    advance(&mut w, ticks);
    assert!(w.unit(enemy).unwrap().hp < 100);
    assert_eq!(
        w.building_attack(PlayerId(0), BuildingKind::Castle)
            .unwrap()
            .min_range,
        1
    );
    w.complete_research(PlayerId(0), Tech::MurderHoles, &mut Vec::new());
    assert_eq!(
        w.building_attack(PlayerId(0), BuildingKind::Castle)
            .unwrap()
            .min_range,
        0
    );
}

#[test]
fn ballistics_leads_a_moving_target_but_unresearched_arrows_miss() {
    fn shot(ballistics: bool) -> u16 {
        let mut w = castle();
        if ballistics {
            w.complete_research(PlayerId(0), Tech::Ballistics, &mut Vec::new());
        }
        let archer = w
            .spawn_kind(PlayerId(0), UnitKind::Crossbowman, at(100, 100))
            .unwrap();
        let victim = w
            .spawn_kind(PlayerId(1), UnitKind::Knight, at(104, 100))
            .unwrap();
        let i = w.lookup[&victim];
        w.issue_move(victim, at(104, 110)).unwrap();
        for _ in 0..2 {
            w.advance();
        }
        w.issue_attack(PlayerId(0), archer, victim).unwrap();
        w.advance();
        let p = *w.projectiles().next().unwrap();
        let delta = w.units[i].state.position.y - w.units[i].state.previous_position.y;
        w.units[i].state.position.y += ((p.impact.0 - w.tick.0) as i32) * delta;
        w.issue_stop(PlayerId(1), victim).unwrap();
        w.tick = aoe_core::Tick(p.impact.0 - 1);
        w.advance();
        w.unit(victim).unwrap().hp
    }
    assert_eq!(shot(false), 100);
    assert_eq!(shot(true), 97);
}

#[test]
fn ballistics_hits_a_scout_after_stop_and_after_arrival() {
    for stop in [true, false] {
        let mut w = castle();
        w.place_town_center(PlayerId(1), TileCoord::new(180, 180))
            .unwrap();
        w.complete_research(PlayerId(1), Tech::FeudalAge, &mut Vec::new());
        w.complete_research(PlayerId(0), Tech::Ballistics, &mut Vec::new());
        let archer = w
            .spawn_kind(PlayerId(0), UnitKind::Crossbowman, at(100, 100))
            .unwrap();
        let victim = w
            .spawn_kind(PlayerId(1), UnitKind::Scout, at(104, 100))
            .unwrap();
        // Isolate Stop/arrival velocity from automatic acquisition on either side.
        w.issue_stance(PlayerId(0), &[archer], aoe_core::CombatStance::NoAttack)
            .unwrap();
        w.issue_stance(PlayerId(1), &[victim], aoe_core::CombatStance::NoAttack)
            .unwrap();
        w.issue_move(victim, at(104, 101)).unwrap();
        if stop {
            w.advance();
            let u = w.unit(victim).unwrap();
            assert_eq!(u.position.y - u.previous_position.y, 79);
            w.issue_stop(PlayerId(1), victim).unwrap();
        } else {
            ticks_until(&mut w, 40, |w| !w.unit(victim).unwrap().moving);
        }
        let stopped = w.unit(victim).unwrap();
        w.issue_attack(PlayerId(0), archer, victim).unwrap();
        w.advance();
        assert_eq!(w.projectiles().next().unwrap().to, stopped.position);
        for _ in 0..60 {
            // Keep the target stationary after hits would otherwise trigger retaliation.
            w.issue_stop(PlayerId(1), victim).unwrap();
            w.advance();
            assert_eq!(w.unit(victim).unwrap().position, stopped.position);
        }
        assert!(
            w.unit(victim).unwrap().hp <= stopped.hp - 6,
            "repeated shots hit"
        );
    }
}

#[test]
fn masonry_preserves_damage_and_applies_to_future_buildings_not_walls() {
    let mut w = castle();
    let house = w
        .insert_building(
            PlayerId(0),
            BuildingKind::House,
            TileRect::from_xywh(90, 90, 2, 2),
            true,
        )
        .unwrap();
    let wall = w
        .insert_building(
            PlayerId(0),
            BuildingKind::PalisadeWall,
            TileRect::from_xywh(94, 90, 1, 1),
            true,
        )
        .unwrap();
    w.economy.buildings.get_mut(&house).unwrap().hp -= 37;
    w.complete_research(PlayerId(0), Tech::Masonry, &mut Vec::new());
    assert_eq!(
        (
            w.building(house).unwrap().hp,
            w.building(house).unwrap().max_hp
        ),
        (953, 990)
    );
    assert_eq!(w.building(wall).unwrap().max_hp, 250);
    let next = w
        .insert_building(
            PlayerId(0),
            BuildingKind::House,
            TileRect::from_xywh(96, 90, 2, 2),
            true,
        )
        .unwrap();
    assert_eq!(w.building(next).unwrap().max_hp, 990);
}

#[test]
fn castle_archer_equipment_refreshes_stationary_units_actual_vision() {
    let mut w = castle();
    let id = w
        .spawn_kind(PlayerId(0), UnitKind::Crossbowman, at(100, 100))
        .unwrap();
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(108, 100)),
        crate::TileVisibility::Unexplored
    );
    w.complete_research(PlayerId(0), Tech::Fletching, &mut Vec::new());
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(108, 100)),
        crate::TileVisibility::Visible
    );
    w.complete_research(PlayerId(0), Tech::BodkinArrow, &mut Vec::new());
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(109, 100)),
        crate::TileVisibility::Visible
    );
    assert_eq!(w.unit_stats(id).unwrap().range, 7);
}

#[test]
fn castle_infantry_uses_its_dat_standard_building_bonus() {
    for (kind, damage) in [(UnitKind::LongSwordsman, 14), (UnitKind::Pikeman, 7)] {
        let mut w = castle();
        let b = w
            .insert_building(
                PlayerId(1),
                BuildingKind::House,
                TileRect::from_xywh(100, 100, 2, 2),
                true,
            )
            .unwrap();
        let unit = w.spawn_kind(PlayerId(0), kind, at(99, 100)).unwrap();
        let hp = w.building(b).unwrap().hp;
        w.issue_attack(PlayerId(0), unit, b).unwrap();
        w.advance();
        // Dark House has -2 melee armour; DAT class 21 adds 3 or 1.
        assert_eq!(hp - w.building(b).unwrap().hp, damage);
    }
}
