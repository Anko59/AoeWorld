//! The staged scenarios on the owner's Anatolia map (centre 39N 34E, 960 km a side, 240:1, 2,000
//! tiles, profile `circa600_v1`): the choke they are played at, and the armies. Chokes are found by
//! `choke::survey` from the walkable tiles of the map and recorded here with what they measured; the
//! staging checks the record against the map it is played on (docs/scale/game-score-chokes.md).
use super::super::choke::Choke;
use super::super::staging::{Army, Side, Staging};
use aoe_simulation::UnitKind;

/// The content hash of the package the choke was surveyed on.
pub(super) const MAP: &str = "ce1c8d7e58352ce6ce98c6c58c05a15b091f2cc151a1f9cc123e338cc2d808e0";

/// A ford over a river: five tiles of shallows in a river that otherwise blocks, with open land on
/// both banks. Found by `--survey-chokes` and measured in full with `--survey-at 1213,1395`; see
/// docs/scale/game-score-chokes.md.
pub(super) const CHOKE: Choke = Choke {
    name: "the ford at 1213,1395",
    saddle: (1213, 1395),
    ends: ((1213, 1391), (1213, 1397)),
    width_tiles: 5,
    length_tiles: 3,
    flow: (1, 0),
    near: (1182, 1393),
    far: (1243, 1394),
    sides: "river on both sides: all seven wall tiles read along the neck are river (class `~`, \
            not walkable)",
    why: "the river runs north to south past the ford for more than sixty tiles and this is its \
          crossing between the two banks within twenty-five tiles: closing the five tiles \
          lengthens the 62 steps between the anchors to 114",
};

/// Eight hundred Militia, as one formation (fifty-six abreast), from the near side to the far side:
/// a front eleven times wider than the five tiles of the ford.
pub(super) const MARCH: Staging = Staging {
    map: MAP,
    choke: &CHOKE,
    armies: &[Army {
        seat: 0,
        from: Side::Near,
        mix: &[(UnitKind::Militia, 800)],
    }],
    attack: false,
};

/// Two armies of four hundred, a mix of melee and ranged, across the same choke from opposite
/// sides, each attack-moving to where the other stands.
pub(super) const BATTLE: Staging = Staging {
    map: MAP,
    choke: &CHOKE,
    armies: &[
        Army {
            seat: 0,
            from: Side::Near,
            mix: &[
                (UnitKind::Militia, 160),
                (UnitKind::Spearman, 80),
                (UnitKind::Archer, 110),
                (UnitKind::Skirmisher, 50),
            ],
        },
        Army {
            seat: 1,
            from: Side::Far,
            mix: &[
                (UnitKind::Militia, 160),
                (UnitKind::Spearman, 80),
                (UnitKind::Archer, 110),
                (UnitKind::Skirmisher, 50),
            ],
        },
    ],
    attack: true,
};
