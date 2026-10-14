//! Small world-anchored patches; scenery has no resource or movement authority.
use super::*;

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
