//! Roads use ordinary construction, never movement collision.
use super::*;

#[test]
fn road_line_is_atomic_dark_age_buildable_and_nonblocking() {
    let (mut w, _, _) = village();
    let worker = w
        .spawn_villager(PlayerId(0), WorldPosition::new(80 * 1024, 80 * 1024))
        .unwrap();
    let stock = w.player_economy(PlayerId(0)).unwrap().stockpile;
    assert!(
        w.issue_build_wall(
            PlayerId(0),
            &[worker],
            BuildingKind::Road,
            TileCoord::new(80, 80),
            TileCoord::new(300, 80),
            false
        )
        .is_err()
    );
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile, stock);
    let ids = w
        .issue_build_wall(
            PlayerId(0),
            &[worker],
            BuildingKind::Road,
            TileCoord::new(80, 80),
            TileCoord::new(84, 80),
            false,
        )
        .unwrap();
    assert_eq!(ids.len(), 5);
    assert!(w.tile_passable(TileCoord::new(82, 80)));
    assert!(
        w.issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::House,
            TileCoord::new(82, 80),
            false
        )
        .is_err()
    );
    for _ in 0..2000 {
        w.advance();
    }
    assert!(ids.iter().all(|id| w.building(*id).unwrap().is_complete()));
    assert_eq!(
        w.player_economy(PlayerId(0)).unwrap().stockpile.wood,
        stock.wood - 5
    );
}

#[test]
fn roads_accelerate_land_units_only_after_completion_and_stop_on_removal() {
    let run = |road, complete, remove| {
        let mut w = world();
        for x in 80..100 {
            if road {
                w.insert_building(
                    PlayerId(0),
                    BuildingKind::Road,
                    TileRect::from_xywh(x, 80, 1, 1),
                    complete,
                )
                .unwrap();
            }
        }
        if remove {
            let ids: Vec<_> = w
                .buildings()
                .filter(|b| b.kind == BuildingKind::Road)
                .map(|b| b.id)
                .collect();
            for id in ids {
                w.remove_building(id);
            }
            assert!(w.economy.roads.is_empty());
        }
        let id = w
            .spawn_villager(
                PlayerId(0),
                WorldPosition::new(80 * 1024 + 512, 80 * 1024 + 512),
            )
            .unwrap();
        w.issue_move(id, WorldPosition::new(98 * 1024 + 512, 80 * 1024 + 512))
            .unwrap();
        for _ in 0..40 {
            w.advance();
        }
        w.unit(id).unwrap().position.x
    };
    let ordinary = run(false, false, false);
    assert_eq!(run(true, false, false), ordinary);
    assert_eq!(run(true, true, true), ordinary);
    let paved = run(true, true, false);
    assert!(paved - (80 * 1024 + 512) > (ordinary - (80 * 1024 + 512)) * 6 / 5);
}

#[test]
fn military_formations_gain_the_road_bonus_too() {
    let run = |roads| {
        let mut w = world();
        if roads {
            for x in 80..100 {
                w.insert_building(
                    PlayerId(0),
                    BuildingKind::Road,
                    TileRect::from_xywh(x, 80, 1, 1),
                    true,
                )
                .unwrap();
            }
        }
        let start = WorldPosition::new(80 * 1024 + 512, 80 * 1024 + 512);
        let id = w.spawn_kind(PlayerId(0), UnitKind::Militia, start).unwrap();
        w.issue_formation_move(
            PlayerId(0),
            &[id],
            aoe_core::Formation::Line,
            WorldPosition::new(98 * 1024 + 512, start.y),
            false,
            false,
        )
        .unwrap();
        for _ in 0..80 {
            w.advance();
        }
        w.unit(id).unwrap().position.x - start.x
    };
    let ordinary = run(false);
    let road = run(true);
    assert!(
        road * 100 > ordinary * 120,
        "formation road displacement {road}, ordinary {ordinary}"
    );
}
