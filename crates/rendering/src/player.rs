//! One palette for bodies, contours and minimap dots. Player IDs are zero based.
use aoe_core::PlayerId;
pub const PLAYER_COLOURS: [[u8; 3]; 8] = [
    [65, 145, 245],
    [245, 65, 65],
    [50, 190, 65],
    [245, 220, 40],
    [40, 220, 220],
    [175, 65, 220],
    [160, 160, 160],
    [245, 145, 40],
];

fn player_index(player: PlayerId, local: PlayerId, chosen: [u8; 3]) -> u8 {
    let local_slot = usize::from(local.0) % 8;
    let chosen_slot = PLAYER_COLOURS
        .iter()
        .position(|c| *c == chosen)
        .unwrap_or(local_slot);
    let slot = usize::from(player.0) % 8;
    let slot = if slot == local_slot {
        chosen_slot
    } else if slot == chosen_slot {
        local_slot
    } else {
        slot
    };
    ((usize::from(player.0) / 8 % 13) * 8 + slot) as u8
}

pub fn player_colour(player: PlayerId, local: PlayerId, chosen: [u8; 3]) -> [u8; 3] {
    palette_colour(player_index(player, local, chosen))
}

fn palette_colour(index: u8) -> [u8; 3] {
    let colour = PLAYER_COLOURS[usize::from(index % 8)];
    let band = usize::from(index / 8);
    colour.map(|c| {
        let c = u32::from(c);
        if band % 2 == 1 {
            ((c * (100 - band.div_ceil(2) as u32 * 9) + 50) / 100) as u8
        } else {
            (c + ((255 - c) * (band as u32 / 2 * 7) + 50) / 100) as u8
        }
    })
}

/// Compose the retained ramp/weight/base-alpha mask before filtering or fog.
/// Base alpha already includes player coverage, so shadows use the same shape.
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn player_texel(base: [u8; 4], mask: [u8; 4], colour: [u8; 3]) -> [u8; 4] {
    let weight = u32::from(mask[1]);
    if weight == 0 {
        return base;
    }
    let shade = 0.65 + f32::from(mask[0].min(7)) / 7.0 * 0.35;
    let foreground = weight * 255;
    let background = u32::from(mask[2]) * (255 - weight);
    let coverage = foreground + background;
    let mut result = base;
    for index in 0..3 {
        let tinted = (f32::from(colour[index]) * shade) as u8;
        result[index] =
            ((u32::from(tinted) * foreground + u32::from(base[index]) * background + coverage / 2)
                / coverage) as u8;
    }
    result
}

#[cfg(test)]
mod tests;

/// Seven palette bits plus movement, contour, disposition and selection flags.
/// Shares the former visual/selection bytes, keeping SceneUnit at 32 bytes.
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct SceneUnitVisual(u16);
impl SceneUnitVisual {
    pub fn new(moving: bool) -> Self {
        Self(u16::from(moving))
    }
    pub fn moving(self) -> bool {
        self.0 & 1 != 0
    }
    pub fn set_moving(&mut self, moving: bool) {
        self.0 = (self.0 & !1) | u16::from(moving);
    }
    pub fn selected(self) -> bool {
        self.0 & 1024 != 0
    }
    pub fn with_selected(mut self, selected: bool) -> Self {
        self.0 = (self.0 & !1024) | u16::from(selected) << 10;
        self
    }
    pub fn with_player(
        self,
        player: Option<PlayerId>,
        local: PlayerId,
        chosen: [u8; 3],
        contour: bool,
    ) -> Self {
        self.with_index(
            player.map(|p| player_index(p, local, chosen)),
            player.is_some_and(|p| p != local),
            contour,
        )
    }
    pub fn with_owner(self, colour: Option<[u8; 3]>, hostile: bool, contour: bool) -> Self {
        self.with_index(
            colour.and_then(|c| (0..104).find(|i| palette_colour(*i) == c)),
            hostile,
            contour,
        )
    }
    fn with_index(mut self, index: Option<u8>, hostile: bool, contour: bool) -> Self {
        self.0 &= 1 | 1024;
        if let Some(index) = index {
            self.0 |= u16::from(index + 1) << 1 | u16::from(contour) << 8 | u16::from(hostile) << 9;
        }
        self
    }
    pub fn colour(self) -> Option<[u8; 3]> {
        ((self.0 >> 1) as u8 & 127)
            .checked_sub(1)
            .map(palette_colour)
    }
    pub fn hostile(self) -> bool {
        self.0 & 512 != 0
    }
    pub fn outline_colour(self) -> Option<[u8; 3]> {
        self.colour().filter(|_| self.0 & 256 != 0)
    }
}
