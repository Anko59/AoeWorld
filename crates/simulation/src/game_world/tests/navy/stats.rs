use super::*;
use crate::{DamageKind, Tech, researched_stats, unit_combat};

/// DE update 188329 records (SiegeEngineers/aoe2techtree data.json, after the
/// Naval Warfare Overhaul): hp, attack, damage kind, armours, reload, speed, los,
/// wood, gold and seconds per ship. The Fishing Ship and Transport are pinned elsewhere.
#[test]
fn navy_stats_match_the_de_data_after_the_naval_overhaul() {
    use DamageKind::{Melee as M, Pierce as P};
    #[allow(clippy::type_complexity)]
    let rows: [(
        UnitKind,
        u16,
        u16,
        DamageKind,
        i16,
        u16,
        u32,
        u32,
        u8,
        u32,
        u32,
        u32,
    ); 15] = [
        (UnitKind::Galley, 110, 6, P, 0, 3, 300, 136, 7, 90, 30, 45),
        (
            UnitKind::WarGalley,
            125,
            7,
            P,
            0,
            4,
            300,
            136,
            8,
            90,
            30,
            27,
        ),
        (UnitKind::Galleon, 155, 9, P, 0, 6, 300, 136, 9, 90, 30, 27),
        (
            UnitKind::FireGalley,
            110,
            2,
            M,
            1,
            4,
            25,
            136,
            5,
            75,
            45,
            49,
        ),
        (UnitKind::FireShip, 120, 3, M, 2, 4, 25, 147, 5, 75, 45, 27),
        (
            UnitKind::FastFireShip,
            140,
            4,
            M,
            3,
            7,
            25,
            156,
            6,
            75,
            45,
            27,
        ),
        (
            UnitKind::DemolitionRaft,
            40,
            75,
            M,
            0,
            0,
            500,
            142,
            6,
            45,
            80,
            45,
        ),
        (
            UnitKind::DemolitionShip,
            50,
            95,
            M,
            1,
            0,
            500,
            152,
            6,
            45,
            80,
            31,
        ),
        (
            UnitKind::HeavyDemolitionShip,
            70,
            120,
            M,
            2,
            0,
            500,
            152,
            6,
            45,
            80,
            31,
        ),
        (
            UnitKind::CannonGalleon,
            120,
            50,
            M,
            0,
            5,
            1000,
            105,
            15,
            200,
            150,
            46,
        ),
        (
            UnitKind::EliteCannonGalleon,
            150,
            60,
            M,
            0,
            6,
            1000,
            105,
            17,
            200,
            150,
            46,
        ),
        (
            UnitKind::CatapultGalleon,
            140,
            30,
            M,
            0,
            6,
            600,
            100,
            14,
            225,
            100,
            55,
        ),
        (UnitKind::Hulk, 85, 4, M, 3, 1, 175, 142, 5, 75, 35, 42),
        (UnitKind::WarHulk, 110, 4, M, 4, 1, 150, 142, 5, 75, 35, 27),
        (UnitKind::Carrack, 135, 4, M, 6, 1, 150, 152, 6, 75, 35, 27),
    ];
    for (kind, hp, attack, damage, melee, pierce, period, speed, los, wood, gold, seconds) in rows {
        let stats = unit_combat(kind);
        assert_eq!(
            (stats.hp, stats.attack, stats.attack_kind),
            (hp, attack, damage),
            "{kind:?}"
        );
        assert_eq!(
            (stats.melee_armour, stats.pierce_armour),
            (melee, pierce),
            "{kind:?}"
        );
        assert_eq!(
            (
                stats.attack_period_centiseconds,
                stats.speed_hundredths,
                stats.los
            ),
            (period, Some(speed), los),
            "{kind:?}"
        );
        let training = stats.training.unwrap();
        assert_eq!(
            (
                training.cost.food,
                training.cost.wood,
                training.cost.gold,
                training.cost.stone,
                training.seconds
            ),
            (0, wood, gold, 0, seconds),
            "{kind:?}"
        );
    }
}

#[test]
fn transport_stats_match_original_dat_record_545() {
    let stats = unit_combat(UnitKind::TransportShip);
    assert_eq!(
        (stats.hp, stats.attack, stats.speed_hundredths, stats.los),
        (100, 0, Some(145), 5)
    );
    assert_eq!((stats.melee_armour, stats.pierce_armour), (4, 8));
    let training = stats.training.unwrap();
    assert_eq!(
        (
            training.cost.food,
            training.cost.wood,
            training.cost.gold,
            training.cost.stone,
            training.seconds
        ),
        (0, 125, 0, 0, 46)
    );
}

#[test]
fn hull_and_blacksmith_techs_change_ship_stats() {
    use UnitKind::*;
    let stats = |kind, techs: &[Tech]| {
        let bits = techs.iter().fold(0, |m, t| m | t.bit());
        researched_stats(kind, true, bits)
    };
    for kind in [
        Galley,
        FireShip,
        Hulk,
        CannonGalleon,
        TransportShip,
        FishingShip,
    ] {
        let base = crate::unit_combat(kind).pierce_armour;
        assert_eq!(
            stats(kind, &[Tech::Careening]).pierce_armour,
            base + 1,
            "{kind:?}"
        );
        assert_eq!(
            stats(kind, &[Tech::Careening, Tech::DryDock]).pierce_armour,
            base + 2,
            "{kind:?}"
        );
    }
    assert_eq!(stats(Knight, &[Tech::Careening]).pierce_armour, 2);
    // Clinker Construction and Carvel Hull: +10% speed each; the Fishing Ship keeps its own.
    assert_eq!(
        stats(Galley, &[Tech::ClinkerConstruction]).speed_hundredths,
        Some(149)
    );
    assert_eq!(
        stats(Galley, &[Tech::ClinkerConstruction, Tech::CarvelHull]).speed_hundredths,
        Some(163)
    );
    // The Hulk line takes the melee Blacksmith line: Forging +1, Iron Casting +1, Blast Furnace +2.
    assert_eq!(stats(Hulk, &[Tech::Forging]).attack, 5);
    assert_eq!(stats(Hulk, &[Tech::Forging, Tech::IronCasting]).attack, 6);
    assert_eq!(
        stats(
            Carrack,
            &[Tech::Forging, Tech::IronCasting, Tech::BlastFurnace]
        )
        .attack,
        8
    );
    assert_eq!(stats(Galley, &[Tech::Forging]).attack, 6);
    // The ranged line takes Fletching; fire ships take Siphons.
    assert_eq!(stats(Galleon, &[Tech::Fletching]).range, 8);
    assert_eq!(stats(FireGalley, &[Tech::Siphons]).attack, 3);
    assert_eq!(stats(Hulk, &[Tech::Siphons]).attack, 4);
}
