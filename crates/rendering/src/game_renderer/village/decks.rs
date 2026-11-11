//! Actors on a bridge stay in front of the flat deck pieces beneath them.
use super::*;

/// Depth of a deck piece above the ground at its tile centre.
pub(super) const DECK_CLEARANCE: f64 = 1.25;
/// Extra depth for an actor standing on a deck tile. A piece is one flat depth
/// while an actor's depth follows its position inside the tile: in the far half
/// the unit was shallower than the deck it stands on, and the deck hid its feet.
/// Half a tile of travel is one depth unit, so one unit more clears every corner
/// (`1.5 - 1.0 + 1.0` against the deck's `1.25`).
const DECK_LIFT: f64 = 1.0;

/// The sorted tiles of the bridge pieces in a scene.
pub(super) struct Decks(Vec<[i32; 2]>);

impl Decks {
    pub(super) fn new(units: &[SceneUnit]) -> Self {
        let mut tiles = units
            .iter()
            .filter(|u| {
                !u.visual.ghost()
                    && u.kind
                        .building_slot()
                        .is_some_and(|slot| (65..=70).contains(&slot))
            })
            .map(|u| tile(u.position))
            .collect::<Vec<_>>();
        tiles.sort_unstable();
        tiles.dedup();
        Self(tiles)
    }

    /// Added to the depth of an actor at `position`.
    pub(super) fn lift(&self, position: [f64; 2]) -> f64 {
        if self.0.binary_search(&tile(position)).is_ok() {
            DECK_LIFT
        } else {
            0.0
        }
    }
}

fn tile(position: [f64; 2]) -> [i32; 2] {
    [position[0].floor() as i32, position[1].floor() as i32]
}
