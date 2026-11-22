//! What each human seat's client shows (#978, 4.2): its view rectangle and
//! whether it draws single units. Set by the host before every tick, beside
//! `set_human_seats`, and recorded with the commands.
//!
//! A unit is watched, and runs the full model, when it is inside the view of
//! a client at unit zoom, plus `WATCH_MARGIN_TILES`. A region stays watched
//! for `LEFT_VIEW_TICKS` after the view moves off it and `LEFT_ZOOM_TICKS`
//! after its client leaves unit zoom, so a pan or one wheel notch out and
//! back changes nothing; each player keeps only the region it left last. A human's own units off screen are not watched; a
//! seat with no subscription watches nothing.
use crate::GameWorld;
use aoe_core::{PlayerId, TileCoord, TileRect};

/// Tiles around a view at unit zoom that are watched too.
pub(crate) const WATCH_MARGIN_TILES: i32 = 16;
/// Reference ticks a region stays watched once its view has moved off it.
const LEFT_VIEW_TICKS: u64 = 40;
/// Reference ticks a region stays watched once its client left unit zoom.
const LEFT_ZOOM_TICKS: u64 = 200;

/// One human client's subscription: `unit_zoom` when it draws single units
/// (`lod_for_viewport` at sprite level), not markers or the map.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HumanView {
    pub player: PlayerId,
    pub region: TileRect,
    pub unit_zoom: bool,
}

impl GameWorld {
    /// The views of every connected human client, in a fixed order. A seat
    /// with no subscription (hidden tab, reconnecting) has none.
    pub fn set_human_views(&mut self, views: &[HumanView]) {
        let now = self.tick.0;
        let old = std::mem::replace(&mut self.vision.human_views, views.to_vec());
        for view in old.iter().filter(|v| v.unit_zoom && !views.contains(v)) {
            let zoomed_out = views
                .iter()
                .any(|v| v.player == view.player && !v.unit_zoom);
            let grace = if zoomed_out {
                LEFT_ZOOM_TICKS
            } else {
                LEFT_VIEW_TICKS
            };
            let until = now.saturating_add(self.config.ticks(grace));
            // One region per player, the one it left last: a panning client
            // changes its view every subscription, and the regions it left
            // before are covered by the margin of the next.
            let recent = &mut self.vision.recent_views;
            match recent
                .iter_mut()
                .find(|(owner, _, _)| *owner == view.player)
            {
                Some(kept) => *kept = (view.player, view.region, until),
                None => recent.push((view.player, view.region, until)),
            }
        }
        self.vision
            .recent_views
            .retain(|&(_, _, until)| until > now);
    }

    pub fn human_views(&self) -> &[HumanView] {
        &self.vision.human_views
    }

    /// Whether a unit on `tile` is watched (see the module comment).
    pub(crate) fn watched_tile(&self, tile: TileCoord) -> bool {
        let now = self.tick.0;
        self.vision
            .human_views
            .iter()
            .filter(|v| v.unit_zoom)
            .map(|v| v.region)
            .chain(
                self.vision
                    .recent_views
                    .iter()
                    .filter(|&&(_, _, until)| until > now)
                    .map(|&(_, region, _)| region),
            )
            .any(|region| within_margin(region, tile))
    }
}

fn within_margin(region: TileRect, tile: TileCoord) -> bool {
    tile.x >= region.min.x - WATCH_MARGIN_TILES
        && tile.x < region.max.x + WATCH_MARGIN_TILES
        && tile.y >= region.min.y - WATCH_MARGIN_TILES
        && tile.y < region.max.y + WATCH_MARGIN_TILES
}
