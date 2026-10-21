use super::military::feudal;
use super::*;
use crate::{Age, BuildingKind, Tech, UnitKind};

#[test]
fn every_blacksmith_technology_changes_only_its_own_stat_and_unit_classes() {
    use crate::researched_stats;
    // Columns: Villager, Scout, Militia, Fishing Ship, Archer, Skirmisher,
    // Spearman, Man-at-Arms. Pin each DE effect independently.
    for (tech, attacks, armours, ranges) in [
        (Tech::Forging, [0, 1, 1, 0, 0, 0, 1, 1], [0; 8], [0; 8]),
        (
            Tech::ScaleMailArmor,
            [0; 8],
            [0, 0, 1, 0, 0, 0, 1, 1],
            [0; 8],
        ),
        (
            Tech::ScaleBardingArmor,
            [0; 8],
            [0, 1, 0, 0, 0, 0, 0, 0],
            [0; 8],
        ),
        (
            Tech::Fletching,
            [0, 0, 0, 0, 1, 1, 0, 0],
            [0; 8],
            [0, 0, 0, 0, 1, 1, 0, 0],
        ),
        (
            Tech::PaddedArcherArmor,
            [0; 8],
            [0, 0, 0, 0, 1, 1, 0, 0],
            [0; 8],
        ),
    ] {
        for (i, kind) in [
            UnitKind::Villager,
            UnitKind::Scout,
            UnitKind::Militia,
            UnitKind::FishingShip,
            UnitKind::Archer,
            UnitKind::Skirmisher,
            UnitKind::Spearman,
            UnitKind::ManAtArms,
        ]
        .into_iter()
        .enumerate()
        {
            let base = researched_stats(kind, true, 0);
            let actual = researched_stats(kind, true, tech.bit());
            let mut expected = base;
            expected.attack += attacks[i];
            expected.melee_armour += armours[i];
            expected.pierce_armour += armours[i] as u16;
            expected.range += ranges[i];
            expected.los += ranges[i];
            assert_eq!(actual, expected, "{tech:?} on {kind:?}");
        }
    }
}

#[test]
fn military_research_enforces_age_cost_cancellation_site_and_exact_duration() {
    let mut w = world();
    w.initialize_village(PlayerId(0), TileCoord::new(60, 60))
        .unwrap();
    feudal(&mut w);
    let blacksmith = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Blacksmith,
            TileRect::new(TileCoord::new(100, 100), TileCoord::new(103, 103)),
            true,
        )
        .unwrap();
    let barracks = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Barracks,
            TileRect::new(TileCoord::new(110, 100), TileCoord::new(113, 103)),
            true,
        )
        .unwrap();
    for (tech, site, food, gold, seconds) in [
        (Tech::Forging, blacksmith, 150, 0, 50),
        (Tech::ScaleMailArmor, blacksmith, 100, 0, 40),
        (Tech::ScaleBardingArmor, blacksmith, 150, 0, 45),
        (Tech::Fletching, blacksmith, 100, 50, 30),
        (Tech::PaddedArcherArmor, blacksmith, 100, 0, 40),
        (Tech::ManAtArms, barracks, 100, 40, 40),
    ] {
        let r = tech.rules();
        assert_eq!(
            (
                r.cost.food,
                r.cost.gold,
                r.cost.wood,
                r.cost.stone,
                r.seconds
            ),
            (food, gold, 0, 0, seconds)
        );
        w.economy.players.get_mut(&PlayerId(0)).unwrap().age = Age::Dark;
        let before = w.canonical_hash();
        assert_eq!(
            w.issue_research(PlayerId(0), site, tech),
            Err(EconomyError::RequiresFeudalAge)
        );
        assert_eq!(w.canonical_hash(), before);
        w.economy.players.get_mut(&PlayerId(0)).unwrap().age = Age::Feudal;
        let wrong = if site == blacksmith {
            barracks
        } else {
            blacksmith
        };
        assert_eq!(
            w.issue_research(PlayerId(0), wrong, tech),
            Err(EconomyError::NotResearchable)
        );
        let before = w.player_economy(PlayerId(0)).unwrap().stockpile;
        w.issue_research(PlayerId(0), site, tech).unwrap();
        let paid = w.player_economy(PlayerId(0)).unwrap().stockpile;
        assert_eq!(
            (paid.food, paid.gold),
            (before.food - food, before.gold - gold)
        );
        w.issue_cancel_research(PlayerId(0), site).unwrap();
        assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile, before);
        w.issue_research(PlayerId(0), site, tech).unwrap();
        advance(&mut w, seconds * 20 - 1);
        assert!(!w.has_researched(PlayerId(0), tech));
        w.advance();
        assert!(w.has_researched(PlayerId(0), tech));
    }
}

#[test]
fn the_starting_scout_and_stable_scout_share_age_stats_and_exact_speed() {
    let mut w = world();
    let scout = w
        .initialize_village(PlayerId(0), TileCoord::new(60, 60))
        .unwrap();
    let s = w.unit_stats(scout).unwrap();
    assert_eq!(
        (s.kind, s.hp, s.attack, s.los, s.speed_hundredths),
        (UnitKind::Scout, 45, 3, 4, Some(120))
    );
    assert_eq!(
        w.movement_speed(UnitKind::Scout, PlayerId(0)),
        (120 * 1024, 100 * 20)
    );
    w.complete_research(PlayerId(0), Tech::FeudalAge, &mut Vec::new());
    let fresh = w
        .spawn_kind(PlayerId(0), UnitKind::Scout, at(100, 100))
        .unwrap();
    for id in [scout, fresh] {
        let s = w.unit_stats(id).unwrap();
        assert_eq!(
            (s.kind, s.attack, s.los, s.speed_hundredths),
            (UnitKind::Scout, 5, 6, Some(155))
        );
    }
    assert_eq!(
        w.movement_speed(UnitKind::Scout, PlayerId(0)),
        (155 * 1024, 100 * 20)
    );
}

#[test]
fn feudal_units_research_and_live_projectiles_replay_deterministically() {
    let run = || {
        let mut w = world();
        feudal(&mut w);
        for tech in [Tech::Forging, Tech::Fletching, Tech::ScaleMailArmor] {
            w.complete_research(PlayerId(0), tech, &mut Vec::new());
        }
        for (kind, victim, y) in [
            (UnitKind::Archer, UnitKind::Militia, 100),
            (UnitKind::Skirmisher, UnitKind::Archer, 110),
            (UnitKind::Spearman, UnitKind::Scout, 120),
        ] {
            let a = w.spawn_kind(PlayerId(0), kind, at(100, y)).unwrap();
            let b = w
                .spawn_kind(
                    PlayerId(1),
                    victim,
                    at(if kind == UnitKind::Spearman { 101 } else { 104 }, y),
                )
                .unwrap();
            w.issue_attack(PlayerId(0), a, b).unwrap();
        }
        (0..200)
            .map(|_| {
                w.advance();
                w.canonical_hash()
            })
            .collect::<Vec<_>>()
    };
    let a = run();
    let b = run();
    assert_eq!(a, b);
    assert_ne!(a.first(), a.last());
}
