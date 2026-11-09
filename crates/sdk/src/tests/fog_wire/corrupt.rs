//! Truncated or corrupted fog never reaches the slot as a broken view. The
//! bytes are whatever the server sent, so no layout is assumed.
use super::*;

/// Drains the page into its world and returns its fog messages as sent bytes.
fn fog_frames(page: &mut Page) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    while let Ok(message) = page.receiver.try_recv() {
        if carries_fog(&message) {
            frames.push(encode_gameplay_server(&message).expect("server fog encodes"));
        }
        page.world.apply(through_the_wire(&message));
    }
    frames
}

/// The slot's fog still answers for every tile of a bounded window.
fn assert_well_formed(world: &World, why: &str) {
    let fog = world.fog();
    let overview = fog.overview();
    assert!(
        overview.len() <= 65_536
            && overview.windows(2).all(|pair| pair[0].0 < pair[1].0)
            && overview.iter().all(|(_, state)| (1..=2).contains(state)),
        "{why}: the overview is out of bounds or order"
    );
    if let Some(region) = fog.region() {
        assert!(
            region.min.x >= 0
                && region.min.y >= 0
                && (1..=512).contains(&region.width())
                && (1..=512).contains(&region.height()),
            "{why}: the window {region:?} is out of bounds"
        );
        sdk_tiles(world, region);
    }
    fog.known_chunks_for_test();
}

#[tokio::test]
async fn truncated_or_corrupted_fog_never_reaches_the_slot_as_a_broken_view() {
    let (service, scouts) = start();
    let mut a = join(&service, None, PlayerId(0), scouts[0]).await;
    // 575 tiles: not a whole number of bytes at two bits a tile.
    let window = TileRect::from_xywh(9, 19, 23, 25);
    service.subscribe(a.session, 1, window).await;
    (a.world.subscription, a.region) = (1, window);
    let nothing = a.world.clone();
    let first = fog_frames(&mut a);
    assert!(!first.is_empty(), "fixture: a first view carries fog");
    assert_in_sync(&service, &a, "first view").await;
    let held = a.world.clone();
    let tiles = sdk_tiles(&held, window);
    let mut next = Vec::new();
    for _ in 0..200 {
        service.tick().await;
        next = fog_frames(&mut a);
        if !next.is_empty() {
            break;
        }
    }
    assert!(!next.is_empty(), "fixture: the scout changed the fog");
    assert_in_sync(&service, &a, "next fog").await;

    // A client holding nothing gets the first view; one holding it, the next.
    for (frames, before) in [(&first, &nothing), (&next, &held)] {
        for frame in frames {
            for cut in 0..frame.len() {
                assert!(
                    decode_gameplay_server(&frame[..cut]).is_err(),
                    "a fog message cut to {cut} of {} bytes decoded",
                    frame.len()
                );
            }
            for at in 0..frame.len() {
                for value in [0x00, 0x01, 0x03, 0x7f, 0x80, 0xff] {
                    let mut bytes = frame.clone();
                    bytes[at] = value;
                    if let Ok(message) = decode_gameplay_server(&bytes) {
                        let mut world = before.clone();
                        world.apply(message);
                        assert_well_formed(&world, &format!("byte {at} set to {value:#04x}"));
                    }
                }
            }
        }
    }

    // No tile state is 255, at one byte or at two bits a tile: a byte forced
    // to 0xff must not give a client another view of the window. A few header
    // bytes may still decode to something valid; hundreds of tile bytes may not.
    let mut accepted = Vec::new();
    for frame in &first {
        for at in 0..frame.len() {
            let mut bytes = frame.clone();
            bytes[at] = 0xff;
            let Ok(message) = decode_gameplay_server(&bytes) else {
                continue;
            };
            let mut world = nothing.clone();
            world.apply(message);
            let region = world.fog().region();
            if region.is_some() && (region != Some(window) || sdk_tiles(&world, window) != tiles) {
                accepted.push(at);
            }
        }
    }
    assert!(
        accepted.len() <= 8,
        "a first view with one byte forced to 0xff was taken as another view at {} \
         positions: {accepted:?}",
        accepted.len()
    );
}
