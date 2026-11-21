//! Small world-anchored scenery patches and loose-rock placement.
use super::*;

/// The native tile diamond is 96 by 48 px. Inverting its projection gives
/// tile offsets (x + 2*y, -x + 2*y) / 96 relative to the centre hotspot.
/// The opaque ground rim at and below the hotspot in the six DE 623 cells
/// reaches 100 and 86 px along those axes. Mirror the reach behind the pile
/// and round out to every tile it overlaps.
const ROCK_REACH_X: i32 = (100 + 48) / 96;
const ROCK_REACH_Y: i32 = (86 + 48) / 96;
pub(super) const ROCK_TILES: usize = ((2 * ROCK_REACH_X + 1) * (2 * ROCK_REACH_Y + 1)) as usize;

pub(super) fn rock_footprint(center: TileCoord) -> impl Iterator<Item = TileCoord> {
    (-ROCK_REACH_Y..=ROCK_REACH_Y).flat_map(move |dy| {
        (-ROCK_REACH_X..=ROCK_REACH_X).map(move |dx| TileCoord::new(center.x + dx, center.y + dy))
    })
}

const CHUNK_SIDE: usize = CHUNK_TILES as usize;
const ANCHOR: u8 = 1;
const COVERED: u8 = 2;

/// Scan the one-tile halo too, since a pile in the next chunk can cover a
/// local tile. These flags are folded into the ordinary terrain and walk bits.
pub(super) struct ChunkRocks {
    origin: TileCoord,
    flags: [u8; CHUNK_SIDE * CHUNK_SIDE],
}

impl ChunkRocks {
    pub(super) fn new(
        origin: TileCoord,
        width: i32,
        anchor: impl Fn(TileCoord) -> Result<bool, EnvironmentPageError>,
        enabled: bool,
    ) -> Result<Self, EnvironmentPageError> {
        let mut rocks = Self {
            origin,
            flags: [0; CHUNK_SIDE * CHUNK_SIDE],
        };
        if enabled {
            let halo = |start: i32, reach: i32| {
                (start - reach).max(0)..start.saturating_add(CHUNK_TILES + reach).min(width)
            };
            for y in halo(origin.y, ROCK_REACH_Y) {
                for x in halo(origin.x, ROCK_REACH_X) {
                    let at = TileCoord::new(x, y);
                    if anchor(at)? {
                        for tile in rock_footprint(at) {
                            if let Some(index) = rocks.index(tile) {
                                rocks.flags[index] |= COVERED;
                            }
                        }
                        if let Some(index) = rocks.index(at) {
                            rocks.flags[index] |= ANCHOR;
                        }
                    }
                }
            }
        }
        Ok(rocks)
    }

    fn index(&self, tile: TileCoord) -> Option<usize> {
        let x = tile.x.wrapping_sub(self.origin.x) as usize;
        let y = tile.y.wrapping_sub(self.origin.y) as usize;
        (x < CHUNK_SIDE && y < CHUNK_SIDE).then(|| y * CHUNK_SIDE + x)
    }

    pub(super) fn at(&self, local_x: i32, local_y: i32) -> (bool, bool) {
        let flags = self.flags[local_y as usize * CHUNK_SIDE + local_x as usize];
        (flags & ANCHOR != 0, flags & COVERED != 0)
    }
}

pub(super) fn allows_dressing(reserved: Reservations) -> bool {
    !reserved.route && !reserved.start && !reserved.resource_approach && !reserved.shore
}

pub(super) fn uncultivated(land_use: LandUse) -> bool {
    !matches!(
        land_use,
        LandUse::Crop | LandUse::Grazing | LandUse::Nonland
    )
}

pub(super) fn rock_anchor<const N: usize, const W: usize>(
    generator: &MapChunkGenerator,
    memo: &memo::Memo<'_, N, W>,
    position: TileCoord,
    composed: &dyn Fn(TileCoord) -> Result<Option<(LandscapeSample, Tile)>, EnvironmentPageError>,
    reserved_at: &dyn Fn(TileCoord) -> Result<Reservations, EnvironmentPageError>,
) -> Result<bool, EnvironmentPageError> {
    let roll = (memo.dressing_roll(position) >> 24) % 1000;
    if roll >= crate::terrain::relief::MAX_ROCK_PER_THOUSAND
        || !generator.landscape_in_bounds(position)
    {
        return Ok(false);
    }
    let Some(base) = memo.base(position)? else {
        return Ok(false);
    };
    if !base.passable || base.water != crate::WaterKind::None {
        return Ok(false);
    }
    let scree = scree_at(generator, memo, position, base, roll)?;
    let cover = generator.ground_cover(position, base.geographic_height_centimeters, base.biome);
    let ordinary = |floor| {
        roll < generator.rock_per_thousand(
            position,
            material(base, floor, cover),
            base.biome,
            false,
        )
    };
    // No forest floor gives the highest ordinary rock chance. Only inspect
    // canopy and reservations when the terrain and roll can produce a pile.
    if !scree && !ordinary(0) {
        return Ok(false);
    }
    let Some((sample, _)) = composed(position)? else {
        return Ok(false);
    };
    if !uncultivated(sample.historical_land_use)
        || sample.density.opening != crate::landscape_patches::Opening::None
        || sample.density.tree
        || sample.density.canopy_per_thousand != 0
        || (!scree
            && sample.density.forest_floor_per_thousand != 0
            && !ordinary(sample.density.forest_floor_per_thousand))
    {
        return Ok(false);
    }
    for at in rock_footprint(position) {
        let Some(tile) = memo.base(at)? else {
            return Ok(false);
        };
        // A resource node under the pile would hide the rock art and sit on
        // blocked ground, so that spot grows no pile.
        if tile.water != crate::WaterKind::None
            || !allows_dressing(reserved_at(at)?)
            || memo.resource(at, tile)?.is_some()
        {
            return Ok(false);
        }
        if at != position
            && let Some((neighbor, _)) = composed(at)?
            && (!uncultivated(neighbor.historical_land_use)
                || neighbor.density.tree
                || neighbor.density.opening != crate::landscape_patches::Opening::None)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn scree_at<const N: usize, const W: usize>(
    generator: &MapChunkGenerator,
    memo: &memo::Memo<'_, N, W>,
    position: TileCoord,
    terrain: Tile,
    roll: u64,
) -> Result<bool, EnvironmentPageError> {
    // Uniform and fallback elevation cannot produce cliff scree.
    let has_relief = generator.provider_environment.as_ref().map_or_else(
        || {
            generator
                .elevation
                .as_ref()
                .is_some_and(|elevation| elevation.has_relief())
        },
        |environment| environment.samples_per_axis > 1,
    );
    if !has_relief
        || roll >= generator.rock_per_thousand(position, terrain.material, terrain.biome, true)
    {
        return Ok(false);
    }
    for (dx, dy) in crate::terrain::relief::SCREE_REACH {
        let near = TileCoord::new(position.x.saturating_add(dx), position.y.saturating_add(dy));
        if memo.base(near)?.is_some_and(|tile| {
            tile.surface.is_cliff()
                && tile.geographic_height_centimeters > terrain.geographic_height_centimeters
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn family(
    generator: &MapChunkGenerator,
    position: TileCoord,
    terrain: Tile,
    appearance: LandscapeAppearance,
    roll: u64,
) -> Option<DecorationFamily> {
    if !soft_ground(terrain.material) || appearance.exposure == NativeExposure::Exposed {
        return None;
    }
    // Closed canopy already supplies the silhouette. Leave its floor quiet.
    if appearance.canopy_strength > 600 {
        return (roll % 1000 < 12).then_some(DecorationFamily::Deadwood);
    }
    if terrain.material == GroundMaterial::Dirt && (roll >> 40).is_multiple_of(2) {
        return None;
    }
    // Scan neighboring cells so a patch crosses cell/chunk boundaries intact.
    // Jittered circular patches leave broad gaps, unlike a tile-wise sprinkle.
    let cx = position.x.div_euclid(6);
    let cy = position.y.div_euclid(6);
    let mut patch = false;
    for y in cy - 1..=cy + 1 {
        for x in cx - 1..=cx + 1 {
            let value = unsigned_noise(generator.geography_key, b"dressing-patches", x, y)
                ^ generator.procedural_seed.rotate_left(7);
            if value % 100 >= 65 {
                continue;
            }
            let dx = i64::from(position.x) - (i64::from(x) * 6 + ((value >> 8) % 6) as i64);
            let dy = i64::from(position.y) - (i64::from(y) * 6 + ((value >> 16) % 6) as i64);
            patch |= dx * dx + dy * dy <= 4;
        }
    }
    if !patch || roll.is_multiple_of(4) {
        return None;
    }
    let family = if appearance.canopy_strength > 0 || (roll >> 32).is_multiple_of(16) {
        DecorationFamily::Deadwood
    } else if matches!(
        terrain.biome,
        Biome::Woodland | Biome::Savanna | Biome::Desert
    ) {
        DecorationFamily::Shrub
    } else {
        DecorationFamily::Grass
    };
    // Rich green ground already supplies busy detail. Retain the wood silhouettes.
    if family == DecorationFamily::Grass
        && terrain.material == GroundMaterial::LushGrass
        && (roll >> 40).is_multiple_of(2)
    {
        return None;
    }
    Some(family)
}
