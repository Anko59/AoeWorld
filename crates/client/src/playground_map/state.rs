use aoe_core::{ScreenPoint, WorldPosition};

#[derive(Clone, Copy)]
pub(crate) struct Sample {
    /// The server tick, kept in 32 bits so a sample stays 16 bytes.
    pub tick: u32,
    pub position: WorldPosition,
    /// Tiles this unit has actually moved since it was first seen; drives
    /// walk animation so that a blocked or slowed unit barely animates.
    pub travel: f32,
}

pub(crate) struct Drag {
    pub start: ScreenPoint,
    pub current: ScreenPoint,
    pub middle: bool,
    pub moved: bool,
    pub center: [f64; 2],
}

impl Drag {
    pub(crate) fn update(&mut self, point: ScreenPoint) {
        self.current = point;
        self.moved |= (point.x - self.start.x).abs() > 4.0 || (point.y - self.start.y).abs() > 4.0;
    }
}
