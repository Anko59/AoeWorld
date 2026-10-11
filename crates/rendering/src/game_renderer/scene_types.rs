//! Backend-neutral scene data; no map transport or simulation ownership.
use aoe_core::EntityId;

#[derive(Clone, Copy)]
pub struct SceneCamera {
    pub center: [f64; 2],
    pub zoom: f64,
    pub viewport: [f64; 2],
    pub focus_elevation_meters: f64,
}

#[derive(Clone, Copy, PartialEq)]
pub struct SceneUnit {
    pub id: EntityId,
    pub position: [f64; 2],
    pub moving: bool,
    pub facing: u8,
    pub selected: bool,
    pub elevation_meters: f64,
}

#[derive(Clone, Copy)]
pub struct SceneTerrain {
    pub position: [f64; 2],
    /// Display material: 0 grass, 1 dry grass, 2 dirt, 3 sand, 4 procedural
    /// rock, 5 water, 6 forest, 7 procedural snow, 8 interim procedural ice,
    /// 9 procedural mud, 10 procedural shallow/shore water. Not recipe identity.
    pub material: u8,
    pub elevation_meters: f64,
    pub surface: SceneTerrainSurface,
    /// None for terrain without landscape metadata (synthetic, non-map scenes);
    /// map chunks always supply an appearance.
    pub appearance: Option<SceneTerrainAppearance>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneTerrainAppearance {
    pub canopy_strength: u16,
    pub floor_strength: u16,
    pub palette: u8,
    pub exposure: u8,
    pub height_band: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneTerrainSurface {
    pub corner_game_height_levels: [i16; 4],
    pub kind: u8,
    pub triangulation: u8,
    pub water: u8,
}

impl SceneTerrainSurface {
    pub const PLATEAU: u8 = 0;
    pub const RAMP: u8 = 1;
    pub const CLIFF: u8 = 2;

    pub const fn flat(elevation_meters: f64) -> Self {
        Self {
            corner_game_height_levels: [elevation_meters as i16; 4],
            kind: Self::PLATEAU,
            triangulation: 0,
            water: 0,
        }
    }
}

/// A resource node, or one piece of scenery when `kind` is a scenery kind.
#[derive(Clone, Copy, PartialEq)]
pub struct SceneResource {
    pub id: u64,
    pub position: [f64; 2],
    pub kind: u8,
    pub visual_variant: u8,
    /// Semantic family, never automatic artwork approval. Zero is generic.
    pub visual_family: u8,
    pub elevation_meters: f64,
}

impl SceneResource {
    /// One horizontal band of a cliff piece. The piece lies on the ground, so
    /// it is drawn as `CLIFF_BANDS` strips that are depth-sorted at their own
    /// ground row: units in front of a row are drawn over it, units behind it
    /// under it. `visual_family` packs the downhill screen octant (bits 0-2),
    /// the band (bits 3-5) and a depth lift (bits 6-7) for ground that rises
    /// above the piece centre inside the row.
    pub const CLIFF: u8 = 16;
    pub const CLIFF_BANDS: u8 = 8;
    /// Ground distance in `x + y` tiles covered by one band.
    pub const CLIFF_BAND_SPAN: f64 = 0.875;
    /// Depth by which a band always covers the ground under it.
    pub const CLIFF_DEPTH_BIAS: f64 = 0.6;
    /// Additional depth per lift level.
    pub const CLIFF_DEPTH_LIFT: f64 = 0.8;

    /// Ground offset on both axes of a band's near edge from the piece centre.
    pub fn cliff_band_offset(band: u8) -> f64 {
        (f64::from(band) + 1.0 - f64::from(Self::CLIFF_BANDS) / 2.0) * Self::CLIFF_BAND_SPAN / 2.0
    }

    pub fn cliff_family(facing: u8, band: u8, lift: u8) -> u8 {
        (facing & 7) | ((band & 7) << 3) | (lift.min(3) << 6)
    }
}

/// Pure visual dressing: never a resource, blocker, gatherable or economy actor.
#[derive(Clone, Copy, PartialEq)]
pub struct SceneDecoration {
    pub position: [f64; 2],
    pub family: u8,
    pub visual_variant: u8,
    pub orientation: u8,
    pub elevation_meters: f64,
}
