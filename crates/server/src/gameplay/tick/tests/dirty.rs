//! Sessions diff only the units their last frame says changed. What each
//! client holds must still equal a full conversion of every unit in view.
use super::scenario::{orders, start};
use crate::gameplay::frame::{Lens, Scratch, UnitFrame};
use aoe_core::{EntityId, TileCoord};
use aoe_protocol::{GameplayServerMessage, GameplayUnitState as Unit};
use std::collections::BTreeMap;

type Units = BTreeMap<EntityId, Unit>;

fn apply(client: &mut Units, message: GameplayServerMessage) {
    match message {
        GameplayServerMessage::Snapshot { units, .. } => {
            *client = units.into_iter().map(|u| (u.id, u)).collect();
        }
        GameplayServerMessage::Tick {
            changed_units,
            removals,
            ..
        } => {
            for unit in changed_units {
                client.insert(unit.id, unit);
            }
            for id in removals {
                client.remove(&id);
            }
        }
        _ => {}
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn every_client_holds_the_full_conversion_of_its_view_each_tick() {
    let mut fixture = start().await;
    let mut clients = vec![Units::new(); fixture.ids.len()];
    // Ticks on which a session's diff skipped units its frame left unchanged.
    let mut skipped = 0;
    for tick in 0..240 {
        {
            let mut world = fixture.service.world.write().await;
            orders(&mut world, tick, &fixture.placed);
        }
        if tick == 100 {
            // A moved view rebuilds its baseline from a snapshot mid-match.
            fixture
                .service
                .subscribe(
                    fixture.ids[2],
                    2,
                    super::scenario::view_region(TileCoord::new(190, 180)),
                )
                .await;
        }
        // Sessions whose baseline is the frame the next one diffs against.
        let last = fixture.service.fanout_probe.last_generation();
        let clean = {
            let sessions = fixture.service.sessions.lock().await;
            fixture
                .ids
                .iter()
                .filter(|id| last.is_some() && sessions[id].resident.frame == last)
                .count()
        };
        skipped += clean;
        fixture.service.tick().await;
        let world = fixture.service.world.read().await;
        let sessions = fixture.service.sessions.lock().await;
        for (index, client) in clients.iter_mut().enumerate() {
            while let Ok(message) = fixture.receivers[index].try_recv() {
                apply(client, message);
            }
            let session = &sessions[&fixture.ids[index]];
            let held = session
                .resident
                .iter()
                .map(|u| (u.id, *u))
                .collect::<Units>();
            assert_eq!(*client, held, "session {index} at tick {tick}");
            let Some(subscription) = session.subscription else {
                continue;
            };
            if session
                .subscription_delivery
                .as_ref()
                .is_some_and(|d| d.pending())
            {
                continue;
            }
            let frame = UnitFrame::build(&world, 1, 0, None).unwrap();
            let lens = Lens {
                viewer: session.viewer,
                primary: session.primary_unit_id,
                region: subscription.region,
            };
            let truth = frame
                .current(&world, &lens, &mut Scratch::default())
                .into_iter()
                .map(|slot| {
                    let state = frame.state(slot).for_viewer(session.viewer);
                    (state.id, state)
                })
                .collect::<Units>();
            assert_eq!(*client, truth, "session {index} at tick {tick}");
        }
    }
    assert!(
        skipped > 1_000,
        "the dirty path was not exercised ({skipped})"
    );
}
