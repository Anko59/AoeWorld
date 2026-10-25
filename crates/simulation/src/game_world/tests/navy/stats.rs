use super::*;
use crate::{DamageKind, unit_combat};

/// Current licensed original VER 5.7 DAT records 539, 529, 527, 545 and 21;
/// SHA-256 a213ed092071e1abd234181158c198f4a83ded970e308bf312eef66d752c3f2e.
/// Pins change when the measured original rules intentionally change. Fractional
/// weapon/blast ranges have their own behavioral tests, not rounded UI pins.
#[test]
fn navy_stats_match_the_original_dat_combat_training_speed_and_los() {
    for (kind, hp, attack, damage, melee, pierce, period, speed, los, wood, gold, seconds) in [
        (
            UnitKind::Galley,
            120,
            6,
            DamageKind::Pierce,
            0,
            6,
            300,
            143,
            7,
            90,
            30,
            36,
        ),
        (
            UnitKind::FireShip,
            100,
            2,
            DamageKind::Pierce,
            0,
            6,
            25,
            135,
            5,
            75,
            45,
            36,
        ),
        (
            UnitKind::DemolitionShip,
            50,
            110,
            DamageKind::Melee,
            0,
            3,
            500,
            160,
            6,
            70,
            50,
            31,
        ),
        (
            UnitKind::WarGalley,
            135,
            7,
            DamageKind::Pierce,
            0,
            6,
            300,
            143,
            8,
            90,
            30,
            36,
        ),
    ] {
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
