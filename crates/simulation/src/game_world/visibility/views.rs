//! What each human seat's client shows (#978, 4.2): its view rectangle and
//! whether it draws single units. Set by the host before every tick, beside
//! `set_human_seats`, and recorded with the commands. Nothing reads it yet.
use crate::GameWorld;
use aoe_core::{PlayerId, TileRect};

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
        self.vision.human_views = views.to_vec();
    }

    pub fn human_views(&self) -> &[HumanView] {
        &self.vision.human_views
    }
}
