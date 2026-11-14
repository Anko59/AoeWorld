//! Generic technologies (content track B): one focused test per mechanic.
//! Costs and times are pinned to the licensed DE DAT (research/generic.rs).
use super::*;
use crate::{Age, TECH_RULES, Tech, TechMask};

mod buildings;
mod monks;
mod units;

fn tile(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

/// A Castle Age player 0 with a deep stockpile, and its Town Center.
fn castle_player() -> (GameWorld, EntityId) {
    let (mut w, _, tc) = village();
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Castle;
    p.researched = Tech::FeudalAge.bit() | Tech::CastleAge.bit();
    p.stockpile = crate::Stockpile {
        food: 20000,
        wood: 20000,
        gold: 20000,
        stone: 20000,
    };
    (w, tc)
}

fn learn(w: &mut GameWorld, tech: Tech) {
    w.complete_research(PlayerId(0), tech, &mut Vec::new());
}

fn researched(w: &GameWorld) -> TechMask {
    w.player_economy(PlayerId(0)).unwrap().researched
}

fn place(w: &mut GameWorld, kind: BuildingKind, slot: i32) -> EntityId {
    w.insert_building(
        PlayerId(0),
        kind,
        aoe_core::TileRect::from_xywh(
            120 + slot % 4 * 8,
            120 + slot / 4 * 8,
            kind.rules().side,
            kind.rules().depth,
        ),
        true,
    )
    .unwrap()
}

#[test]
fn every_technology_has_its_own_bit_and_table_row() {
    assert!(TECH_RULES.len() <= 128, "the researched mask is 128 bits");
    for (i, rules) in TECH_RULES.iter().enumerate() {
        assert_eq!(rules.tech as usize, i, "{:?} is out of order", rules.tech);
        assert_eq!(rules.tech.bit().count_ones(), 1);
        assert_eq!(rules.tech.rules().tech, rules.tech);
    }
}

#[test]
fn generic_rules_match_the_licensed_dat_costs_times_buildings_and_ages() {
    use BuildingKind as B;
    let row = |t: Tech| {
        let r = t.rules();
        (
            r.cost.food,
            r.cost.wood,
            r.cost.gold,
            r.cost.stone,
            r.seconds,
            r.building,
            r.age,
        )
    };
    for (tech, expected) in [
        (Tech::Squires, (100, 0, 0, 0, 40, B::Barracks, Age::Castle)),
        (Tech::Arson, (75, 0, 25, 0, 25, B::Barracks, Age::Feudal)),
        (
            Tech::Gambesons,
            (50, 0, 100, 0, 25, B::Barracks, Age::Castle),
        ),
        (
            Tech::ThumbRing,
            (300, 250, 0, 0, 45, B::ArcheryRange, Age::Castle),
        ),
        (
            Tech::ParthianTactics,
            (200, 0, 250, 0, 65, B::ArcheryRange, Age::Imperial),
        ),
        (
            Tech::Bloodlines,
            (150, 0, 100, 0, 50, B::Stable, Age::Feudal),
        ),
        (Tech::Husbandry, (150, 0, 0, 0, 40, B::Stable, Age::Castle)),
        (
            Tech::Architecture,
            (300, 200, 0, 0, 70, B::University, Age::Imperial),
        ),
        (
            Tech::SiegeEngineers,
            (500, 600, 0, 0, 45, B::University, Age::Imperial),
        ),
        (
            Tech::HeatedShot,
            (350, 0, 100, 0, 30, B::University, Age::Castle),
        ),
        (
            Tech::Arrowslits,
            (250, 250, 0, 0, 25, B::University, Age::Imperial),
        ),
        (
            Tech::FortifiedWall,
            (200, 100, 0, 0, 50, B::University, Age::Castle),
        ),
        (
            Tech::GuardTower,
            (100, 250, 0, 0, 30, B::University, Age::Castle),
        ),
        (
            Tech::Keep,
            (500, 350, 0, 0, 75, B::University, Age::Imperial),
        ),
        (
            Tech::TreadmillCrane,
            (0, 200, 0, 50, 20, B::University, Age::Castle),
        ),
        (
            Tech::Devotion,
            (100, 0, 200, 0, 40, B::Monastery, Age::Castle),
        ),
        (
            Tech::Faith,
            (550, 0, 750, 0, 60, B::Monastery, Age::Imperial),
        ),
        (Tech::Fervor, (0, 0, 140, 0, 50, B::Monastery, Age::Castle)),
        (
            Tech::Sanctity,
            (0, 0, 175, 0, 60, B::Monastery, Age::Castle),
        ),
        (
            Tech::HerbalMedicine,
            (0, 0, 200, 0, 35, B::Monastery, Age::Castle),
        ),
        (
            Tech::Theocracy,
            (0, 0, 200, 0, 75, B::Monastery, Age::Imperial),
        ),
        (
            Tech::Illumination,
            (0, 0, 120, 0, 65, B::Monastery, Age::Imperial),
        ),
        (
            Tech::BlockPrinting,
            (0, 0, 200, 0, 55, B::Monastery, Age::Imperial),
        ),
        (
            Tech::Atonement,
            (0, 0, 325, 0, 40, B::Monastery, Age::Castle),
        ),
        (
            Tech::Redemption,
            (0, 0, 475, 0, 50, B::Monastery, Age::Castle),
        ),
        (Tech::Heresy, (0, 0, 1000, 0, 60, B::Monastery, Age::Castle)),
        (
            Tech::Conscription,
            (150, 0, 150, 0, 60, B::Castle, Age::Imperial),
        ),
        (
            Tech::Hoardings,
            (400, 400, 0, 0, 75, B::Castle, Age::Imperial),
        ),
        (
            Tech::Sappers,
            (400, 200, 0, 0, 10, B::Castle, Age::Imperial),
        ),
        (Tech::Spies, (0, 0, 200, 0, 1, B::Castle, Age::Imperial)),
        (Tech::Caravan, (200, 0, 200, 0, 40, B::Market, Age::Castle)),
        (Tech::Guilds, (300, 0, 200, 0, 50, B::Market, Age::Imperial)),
        (
            Tech::Banking,
            (300, 0, 200, 0, 70, B::Market, Age::Imperial),
        ),
        (Tech::Coinage, (200, 0, 100, 0, 70, B::Market, Age::Castle)),
    ] {
        assert_eq!(row(tech), expected, "{tech:?}");
    }
}

#[test]
fn generic_techs_are_researched_at_their_building_with_age_and_prerequisite() {
    let (mut w, _) = castle_player();
    let university = place(&mut w, BuildingKind::University, 0);
    let monastery = place(&mut w, BuildingKind::Monastery, 1);
    // Faith needs Imperial Age and Devotion; Devotion needs only Castle Age.
    assert_eq!(
        w.issue_research(PlayerId(0), monastery, Tech::Faith),
        Err(EconomyError::RequiresImperialAge)
    );
    assert_eq!(
        w.issue_research(PlayerId(0), university, Tech::Devotion),
        Err(EconomyError::NotResearchable),
        "Devotion belongs to the Monastery"
    );
    w.issue_research(PlayerId(0), monastery, Tech::Devotion)
        .unwrap();
    let gold = w.player_economy(PlayerId(0)).unwrap().stockpile.gold;
    assert_eq!(gold, 20000 - 200);
    let ticks = 40 * w.config().tick_hz;
    advance(&mut w, ticks);
    assert!(w.has_researched(PlayerId(0), Tech::Devotion));
    learn(&mut w, Tech::ImperialAge);
    w.issue_research(PlayerId(0), monastery, Tech::Faith)
        .unwrap();
    // Keep needs the Guard Tower.
    assert_eq!(
        w.issue_research(PlayerId(0), university, Tech::Keep),
        Err(EconomyError::TechRequirementMissing)
    );
}
