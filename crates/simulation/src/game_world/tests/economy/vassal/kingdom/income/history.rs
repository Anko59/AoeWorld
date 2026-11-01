//! Bucket timing affects future allocation even when current income is equal.
use super::*;
use crate::CarriedResource;

fn empty_household() -> GameWorld {
    let mut w = world();
    w.insert_building(
        PlayerId(0),
        BuildingKind::House,
        aoe_core::TileRect::from_xywh(20, 20, 2, 2),
        true,
    )
    .unwrap();
    reserves(&mut w, 0, 0, 0, 0);
    w
}

#[test]
fn equal_current_income_with_different_live_deposit_timing_has_a_different_canonical_hash() {
    let mut older = empty_household();
    let mut younger = empty_household();
    assert_eq!(older.canonical_hash(), younger.canonical_hash());
    let hz = older.config().tick_hz;
    for step in 0..10 * hz {
        if step == 0 {
            let at = older.tick().0;
            assert!(older.economy.credit_delivery(
                PlayerId(0),
                CarriedResource {
                    kind: ResourceKind::Food,
                    amount: 50
                },
                at,
                hz
            ));
        }
        if step == 5 * hz {
            let at = younger.tick().0;
            assert!(younger.economy.credit_delivery(
                PlayerId(0),
                CarriedResource {
                    kind: ResourceKind::Food,
                    amount: 50
                },
                at,
                hz
            ));
        }
        older.advance();
        younger.advance();
    }
    assert_eq!(older.tick(), younger.tick());
    assert_eq!(
        older.player_economy(PlayerId(0)),
        younger.player_economy(PlayerId(0))
    );
    assert_eq!(resource_stock(&older, ResourceKind::Food), 50);
    let recent = |w: &GameWorld| w.economy.income[&PlayerId(0)].recent(w.tick().0, hz);
    assert_eq!(recent(&older), [50, 0, 0, 0]);
    assert_eq!(recent(&older), recent(&younger));
    assert_eq!(older.unit_count(), 0);
    assert_eq!(younger.unit_count(), 0);
    // Prove all other canonical state is equal, then restore the production
    // histories. No replacement history implementation or pinned hash is used.
    let old_history = std::mem::take(&mut older.economy.income);
    let young_history = std::mem::take(&mut younger.economy.income);
    assert_eq!(older.canonical_hash(), younger.canonical_hash());
    older.economy.income = old_history;
    younger.economy.income = young_history;
    assert_ne!(
        older.canonical_hash(),
        younger.canonical_hash(),
        "equal current totals cannot erase timing that controls future expiry"
    );
    advance(&mut older, 50 * hz);
    advance(&mut younger, 50 * hz);
    assert_eq!(
        older.player_economy(PlayerId(0)),
        younger.player_economy(PlayerId(0))
    );
    assert_eq!(recent(&older), [0, 0, 0, 0]);
    assert_eq!(
        recent(&younger),
        [50, 0, 0, 0],
        "the later real credit remains live when the earlier bucket expires"
    );
}
