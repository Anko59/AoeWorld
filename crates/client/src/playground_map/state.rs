use aoe_core::{ScreenPoint, WorldPosition};

#[derive(Clone, Copy)]
pub(crate) struct Sample {
    pub tick: u64,
    pub position: WorldPosition,
}

pub(crate) struct Drag {
    pub start: ScreenPoint,
    pub current: ScreenPoint,
    pub middle: bool,
    pub center: [f64; 2],
}
