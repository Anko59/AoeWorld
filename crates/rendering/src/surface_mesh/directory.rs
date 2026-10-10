//! Terrain frame directory: world tile to native atlas frame, shared by every backend.
use crate::{AtlasAddress, GameArt};

#[cfg(test)]
pub(crate) mod fixture;
#[cfg(test)]
mod tests;

/// Texture mode of a face that spans several world tiles.
pub(crate) const TILED: u8 = 6;

/// Not implemented yet: the tests in this commit specify the behaviour.
pub fn write_terrain_directory(_art: &GameArt, _atlas: &mut [u8]) -> Result<(), String> {
    Ok(())
}

pub(crate) fn frame(_atlas: &[u8], _material: u8, _tile: [i32; 2]) -> Option<AtlasAddress> {
    None
}
