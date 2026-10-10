//! Composed landscape scene data: physical terrain plus its ecological appearance,
//! resources with visual families and nonblocking decorations.
use super::{ResourceNode, Tile};
use aoe_core::TileCoord;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandscapeTile {
    pub tile: TileCoord,
    pub terrain: Tile,
    pub appearance: LandscapeAppearance,
}

/// Coherent descriptor strengths use per-thousand integers, not source evidence.
/// The chunk codec requires equal canopy/floor strengths, each at most 1000;
/// Savanna/Treeless ecology modes require both forest strengths to be zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandscapeAppearance {
    pub canopy_strength: u16,
    pub floor_strength: u16,
    pub palette: EcologicalPalette,
    pub exposure: NativeExposure,
    pub height_band: NativeHeightBand,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum EcologicalPalette {
    Temperate,
    Boreal,
    Tropical,
    DryScrub,
    Savanna,
    Treeless,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum NativeExposure {
    Sheltered,
    Open,
    Exposed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum NativeHeightBand {
    Lowland,
    Montane,
    Subalpine,
    Alpine,
    Nival,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandscapeResource {
    pub node: ResourceNode,
    pub visual_family: ResourceVisualFamily,
}

/// Semantic selection only: these names do not approve or promote any art.
/// Trees always carry a specific family; other resources are `Generic`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ResourceVisualFamily {
    Generic,
    Broadleaf,
    Conifer,
    DryScrub,
    Tropical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DecorationFamily {
    Shrub,
    Grass,
    Stone,
    Deadwood,
}

/// Nonblocking visual dressing, with no resource identity, amount or simulation
/// API. Orientation is one of eight directions (0..=7); variant is opaque.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandscapeDecoration {
    pub tile: TileCoord,
    pub family: DecorationFamily,
    pub variant: u8,
    pub orientation: u8,
}

/// Extensions live with their objects, avoiding parallel-array length contracts.
/// Explicit coordinates represent partial/sparse chunks in strict row-major
/// order. The codec allows one resource and one decoration per present tile
/// cell, including their overlap.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandscapeChunk {
    pub x: i32,
    pub y: i32,
    pub tiles: Vec<LandscapeTile>,
    pub resources: Vec<LandscapeResource>,
    pub decorations: Vec<LandscapeDecoration>,
}
