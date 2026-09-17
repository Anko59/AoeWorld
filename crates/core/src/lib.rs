//! Configuration-independent values shared by the synthetic application.
use serde::{Deserialize, Serialize};

pub const CHUNK_SIZE: i32 = 64;
pub const WORLD_LIMIT: i32 = 16_384;

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct EntityId(pub u32);

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct PlayerId(pub u16);

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct Tick(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct Seed(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u16,
    pub height: u16,
}

impl Region {
    pub fn valid(self, world_size: i32) -> bool {
        self.width > 0
            && self.height > 0
            && self.width <= 512
            && self.height <= 512
            && self.x >= 0
            && self.y >= 0
            && self
                .x
                .checked_add(i32::from(self.width))
                .is_some_and(|v| v <= world_size)
            && self
                .y
                .checked_add(i32::from(self.height))
                .is_some_and(|v| v <= world_size)
    }

    pub fn contains(self, p: Position) -> bool {
        p.x >= self.x
            && p.y >= self.y
            && p.x < self.x + i32::from(self.width)
            && p.y < self.y + i32::from(self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_are_bounded() {
        assert!(
            Region {
                x: 0,
                y: 0,
                width: 512,
                height: 512
            }
            .valid(WORLD_LIMIT)
        );
        assert!(
            !Region {
                x: -1,
                y: 0,
                width: 1,
                height: 1
            }
            .valid(WORLD_LIMIT)
        );
        assert!(
            !Region {
                x: i32::MAX,
                y: 0,
                width: 1,
                height: 1
            }
            .valid(WORLD_LIMIT)
        );
    }
}
