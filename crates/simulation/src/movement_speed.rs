use aoe_core::{FIXED_SUBUNITS_PER_TILE, WorldConfig};
use aoe_map::{CAVALRY_METERS_PER_SECOND, GAME_TILE_METERS};

pub(crate) fn cavalry_config(config: WorldConfig) -> WorldConfig {
    physical_speed_config(config, CAVALRY_METERS_PER_SECOND, 1)
}

pub(crate) fn physical_speed_config(
    mut config: WorldConfig,
    meters_per_second_numerator: u32,
    meters_per_second_denominator: u32,
) -> WorldConfig {
    let numerator = u64::from(meters_per_second_numerator)
        * u64::try_from(FIXED_SUBUNITS_PER_TILE).unwrap_or_default();
    let denominator = u64::from(GAME_TILE_METERS)
        * u64::from(meters_per_second_denominator)
        * u64::from(config.tick_hz);
    let divisor = gcd(numerator, denominator);
    config.move_speed_subunits_per_tick = i32::try_from(numerator / divisor).unwrap_or(i32::MAX);
    config.move_speed_subunits_per_tick_denominator = denominator / divisor;
    config
}

fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}
