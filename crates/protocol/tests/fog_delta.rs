//! Fog deltas: what a client that already holds the view of its subscription
//! is sent when that view changes, instead of the whole view again.
//!
//! These tests ask `aoe-protocol` for this API; the encoding stays free:
//! - `FogState::delta_to(&self, fresh: &FogState) -> Option<FogDelta>`: the
//!   changes from the held view `self` to `fresh`. `None` when `fresh` is
//!   another subscription revision, window or fog mode: it is sent whole.
//! - `FogDelta::is_empty(&self) -> bool`: nothing changed, nothing is sent.
//! - `FogState::apply(&mut self, delta: &FogDelta) -> Result<(), E>` with
//!   `E: Debug`: the client's side. An error leaves the view as it was.
//! - `GameplayServerMessage::FogDelta(FogDelta)`.
use aoe_core::{Tick, TileRect};
use aoe_protocol::{
    FogDelta, FogState, GameplayServerMessage as M, decode_gameplay_server, encode_gameplay_server,
};

const WINDOW: TileRect = TileRect::from_xywh(120, 300, 96, 96);
const TILES: usize = 96 * 96;

/// Deterministic noise over the three states.
fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % 3) as u8
        })
        .collect()
}

fn view(region: TileRect, tiles: Vec<u8>, overview: Vec<(u16, u8)>) -> FogState {
    FogState {
        revision: 3,
        tick: Tick(41),
        region,
        disabled: false,
        tiles,
        overview,
    }
}

/// The same fog, whatever tick it was built at.
fn assert_same(held: &FogState, fresh: &FogState, when: &str) {
    assert_eq!(
        (held.revision, held.region, held.disabled, held.tiles.len()),
        (
            fresh.revision,
            fresh.region,
            fresh.disabled,
            fresh.tiles.len()
        ),
        "{when}"
    );
    let tile = held
        .tiles
        .iter()
        .zip(&fresh.tiles)
        .position(|(a, b)| a != b);
    assert_eq!(tile, None, "{when}: the first tile that differs");
    assert!(
        held.overview == fresh.overview,
        "{when}: the strategic overview differs"
    );
}

/// A delta as the client receives it, and its bytes on the wire.
fn wire(delta: FogDelta) -> (FogDelta, Vec<u8>) {
    let bytes = encode_gameplay_server(&M::FogDelta(delta)).expect("a built delta encodes");
    match decode_gameplay_server(&bytes).expect("and decodes") {
        M::FogDelta(delta) => (delta, bytes),
        other => panic!("a fog delta decoded as {other:?}"),
    }
}

/// One send: the server diffs the fresh view against what the client holds,
/// the client applies what arrives. Returns the bytes sent, none when nothing
/// changed.
fn send(held: &mut FogState, fresh: &FogState, when: &str) -> Vec<u8> {
    let delta = held
        .delta_to(fresh)
        .unwrap_or_else(|| panic!("{when}: the same revision, window and mode took a whole view"));
    if delta.is_empty() {
        assert_same(held, fresh, when);
        return Vec::new();
    }
    let (delta, bytes) = wire(delta);
    held.apply(&delta)
        .unwrap_or_else(|error| panic!("{when}: the delta was refused: {error:?}"));
    assert_same(held, fresh, when);
    bytes
}

fn refused(mut client: FogState, delta: &FogDelta, why: &str) {
    let before = client.clone();
    assert!(client.apply(delta).is_err(), "{why}: the delta was applied");
    assert!(client == before, "{why}: a refused delta changed the view");
}

#[test]
fn deltas_bring_the_held_view_to_the_fresh_one_for_any_window_and_content() {
    let shapes = [
        (1, 1),
        (2, 1),
        (1, 3),
        (5, 3),
        (7, 9),
        (96, 96),
        (97, 95),
        (512, 3),
    ];
    for (width, height) in shapes {
        let region = TileRect::from_xywh(120, 300, width, height);
        let len = (width * height) as usize;
        let mut three_tiles = noise(2, len);
        for index in [0, len / 2, len - 1] {
            three_tiles[index] = (three_tiles[index] + 1) % 3;
        }
        let some = vec![(5, 1), (9, 2), (65_535, 1)];
        let steps = [
            (noise(1, len), vec![(5, 2)]),
            (noise(1, len), some.clone()),
            (noise(2, len), some.clone()),
            (three_tiles, some),
            (vec![2; len], vec![(0, 2), (5, 2), (9, 2), (65_535, 2)]),
            (vec![1; len], vec![(0, 1), (5, 1), (9, 1), (65_535, 1)]),
            (vec![0; len], vec![(0, 1), (5, 1), (9, 1), (65_535, 1)]),
        ];
        let mut held = view(region, vec![0; len], Vec::new());
        for (step, (tiles, overview)) in steps.into_iter().enumerate() {
            let when = format!("{width} x {height}, step {step}");
            send(&mut held, &view(region, tiles, overview), &when);
        }
    }
}

#[test]
fn an_unchanged_view_is_an_empty_delta_and_a_changed_one_is_not() {
    let held = view(WINDOW, noise(3, TILES), vec![(5, 1)]);
    let later = FogState {
        tick: Tick(99),
        ..held.clone()
    };
    let delta = held.delta_to(&later).expect("the same view");
    assert!(delta.is_empty(), "only the tick moved");
    let mut tile = later.clone();
    tile.tiles[17] = (tile.tiles[17] + 1) % 3;
    let delta = held.delta_to(&tile).expect("the same window");
    assert!(!delta.is_empty(), "a tile changed");
    let pixel = FogState {
        overview: vec![(5, 2)],
        ..later
    };
    let delta = held.delta_to(&pixel).expect("the same window");
    assert!(!delta.is_empty(), "a strategic pixel changed");
}

#[test]
fn another_revision_window_or_mode_needs_the_whole_view() {
    let held = view(WINDOW, noise(3, TILES), Vec::new());
    let smaller = TileRect::from_xywh(120, 300, 96, 95);
    let others = [
        (
            "revision",
            FogState {
                revision: 4,
                ..held.clone()
            },
        ),
        (
            "window",
            FogState {
                region: TileRect::from_xywh(121, 300, 96, 96),
                ..held.clone()
            },
        ),
        ("size", view(smaller, noise(3, 96 * 95), Vec::new())),
        (
            "fog mode",
            FogState {
                disabled: true,
                tiles: Vec::new(),
                ..held.clone()
            },
        ),
    ];
    for (what, other) in &others {
        assert!(
            held.delta_to(other).is_none(),
            "another {what}: changes instead of the whole view"
        );
        assert!(
            other.delta_to(&held).is_none(),
            "another {what}, the other way: changes instead of the whole view"
        );
    }
}

/// A 96 x 96 window where one observer sees a disc around `centre`; what
/// `before` saw and this one does not is explored.
fn sight(before: Option<&[u8]>, centre: (i32, i32), radius: i32) -> Vec<u8> {
    let mut tiles = match before {
        Some(tiles) => tiles.iter().map(|state| (*state).min(1)).collect(),
        None => vec![0; TILES],
    };
    for y in 0..96 {
        for x in 0..96 {
            let (dx, dy) = (x - centre.0, y - centre.1);
            if dx * dx + dy * dy <= radius * radius {
                tiles[(y * 96 + x) as usize] = 2;
            }
        }
    }
    tiles
}

#[test]
fn one_observer_moving_one_tile_costs_under_200_bytes() {
    // The sight of a scout (4 tiles) and the longest of a walking unit (8).
    let costs = [4, 8].map(|radius| {
        let before = sight(None, (40, 40), radius);
        let after = sight(Some(&before), (41, 40), radius);
        let mut held = view(WINDOW, before, vec![(5, 2)]);
        let fresh = view(WINDOW, after, vec![(5, 2)]);
        (radius, send(&mut held, &fresh, "one tile east").len())
    });
    assert!(
        costs.iter().all(|(_, bytes)| (1..=200).contains(bytes)),
        "bytes per one-tile move as (sight in tiles, bytes): {costs:?}; budget 200"
    );
}

#[test]
fn an_unchanged_overview_adds_zero_bytes() {
    let before = noise(3, TILES);
    let mut after = before.clone();
    after[4_000] = (after[4_000] + 1) % 3;
    let every_pixel = (0..=u16::MAX)
        .map(|pixel| (pixel, 1 + (pixel % 2) as u8))
        .collect::<Vec<_>>();
    let [none, all] = [Vec::new(), every_pixel].map(|overview| {
        let mut held = view(WINDOW, before.clone(), overview.clone());
        let fresh = view(WINDOW, after.clone(), overview);
        send(&mut held, &fresh, "one tile changed").len()
    });
    assert!(none > 0, "a tile change is sent");
    assert_eq!(
        all, none,
        "bytes for the same tile change while holding 65,536 strategic pixels (left) \
         and none (right)"
    );
}

#[test]
fn a_changed_strategic_pixel_costs_a_few_bytes_not_the_whole_overview() {
    let tiles = noise(3, TILES);
    let mut overview = (0..=u16::MAX)
        .filter(|pixel| *pixel != 40_000)
        .map(|pixel| (pixel, 1))
        .collect::<Vec<_>>();
    let mut held = view(WINDOW, tiles.clone(), overview.clone());
    overview[123].1 = 2;
    let fresh = view(WINDOW, tiles.clone(), overview.clone());
    let turned = send(&mut held, &fresh, "a pixel turned visible").len();
    overview.insert(40_000, (40_000, 2));
    let fresh = view(WINDOW, tiles, overview);
    let appeared = send(&mut held, &fresh, "a pixel appeared").len();
    assert!(
        (1..=40).contains(&turned) && (1..=40).contains(&appeared),
        "one strategic pixel of 65,536 changed: {turned} bytes when it turned visible, \
         {appeared} when it appeared; budget 40"
    );
}

#[test]
fn a_strategic_pixel_that_goes_black_is_exact_or_needs_the_whole_view() {
    // The simulation never unlists a pixel while fog stays on, but a delta
    // must not leave one behind either.
    let held = view(WINDOW, noise(3, TILES), vec![(5, 1), (9, 2)]);
    let fresh = view(WINDOW, noise(3, TILES), vec![(9, 2)]);
    if let Some(delta) = held.delta_to(&fresh) {
        assert!(!delta.is_empty(), "a pixel went black");
        let mut client = held.clone();
        client
            .apply(&wire(delta).0)
            .expect("the view it was made for");
        assert_same(&client, &fresh, "a pixel went black");
    }
}

#[test]
fn a_delta_applies_only_to_the_view_it_was_made_for() {
    let held = view(WINDOW, noise(3, TILES), vec![(5, 1)]);
    let mut fresh = held.clone();
    for index in [0, TILES - 1] {
        fresh.tiles[index] = (fresh.tiles[index] + 1) % 3;
    }
    let (delta, _) = wire(held.delta_to(&fresh).expect("the same window"));
    let other_revision = FogState {
        revision: 4,
        ..held.clone()
    };
    refused(other_revision, &delta, "another revision");
    // The delta's last change lies outside a window one row shorter.
    let smaller = TileRect::from_xywh(120, 300, 96, 95);
    let shorter = view(smaller, noise(3, 96 * 95), vec![(5, 1)]);
    refused(shorter, &delta, "a smaller window");
    let fog_off = FogState {
        disabled: true,
        tiles: Vec::new(),
        overview: Vec::new(),
        ..held.clone()
    };
    refused(fog_off, &delta, "fog switched off");
    let mut client = held.clone();
    client.apply(&delta).expect("the view it was made for");
    assert_same(&client, &fresh, "the view it was made for");
}

#[test]
fn a_corrupted_or_truncated_delta_never_leaves_a_broken_view() {
    let region = TileRect::from_xywh(120, 300, 7, 9);
    let held = view(region, noise(5, 63), vec![(7, 2), (40_000, 1)]);
    let fresh = view(region, noise(6, 63), vec![(7, 1), (300, 2), (40_000, 1)]);
    let (_, bytes) = wire(held.delta_to(&fresh).expect("the same window"));
    for cut in 0..bytes.len() {
        assert!(
            decode_gameplay_server(&bytes[..cut]).is_err(),
            "a delta cut to {cut} of {} bytes decoded",
            bytes.len()
        );
    }
    for at in 0..bytes.len() {
        for value in 0..=u8::MAX {
            let mut corrupted = bytes.clone();
            corrupted[at] = value;
            let Ok(M::FogDelta(delta)) = decode_gameplay_server(&corrupted) else {
                continue;
            };
            let why = format!("byte {at} set to {value:#04x}");
            let mut client = held.clone();
            if client.apply(&delta).is_ok() {
                assert!(client.valid(), "{why}: applied, and the view is invalid");
                assert_eq!(
                    (client.revision, client.region, client.disabled),
                    (held.revision, held.region, held.disabled),
                    "{why}: applied, and the view is another subscription's"
                );
            } else {
                assert!(client == held, "{why}: a refused delta changed the view");
            }
        }
    }
}
