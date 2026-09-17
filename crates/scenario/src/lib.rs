//! Versioned synthetic workload definitions; no gameplay claim is implied.
use aoe_core::Seed;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Scenario {
    pub version: u16,
    pub name: &'static str,
    pub seed: Seed,
    pub world_size: i32,
    pub active_extent: i32,
    pub players: u16,
    pub entities: u32,
    pub hotspot_entities: u32,
}

pub const SMOKE: Scenario = Scenario {
    version: 1,
    name: "smoke",
    seed: Seed(7),
    world_size: 1_024,
    active_extent: 1_024,
    players: 8,
    entities: 8_000,
    hotspot_entities: 0,
};
pub const TARGET_DISTRIBUTED: Scenario = Scenario {
    version: 1,
    name: "target-distributed",
    seed: Seed(17),
    world_size: 16_384,
    active_extent: 16_384,
    players: 64,
    entities: 128_000,
    hotspot_entities: 0,
};
pub const TARGET_HOTSPOT: Scenario = Scenario {
    version: 1,
    name: "target-hotspot",
    seed: Seed(19),
    world_size: 16_384,
    active_extent: 16_384,
    players: 64,
    entities: 128_000,
    hotspot_entities: 10_000,
};
pub const NETWORK_PRESSURE: Scenario = Scenario {
    version: 1,
    name: "network-pressure",
    seed: Seed(19),
    world_size: 16_384,
    active_extent: 16_384,
    players: 64,
    entities: 128_000,
    hotspot_entities: 10_000,
};

pub const POPULATION_8K: Scenario = Scenario {
    version: 1,
    name: "population-8k",
    seed: Seed(23),
    world_size: 16_384,
    active_extent: 16_384,
    players: 4,
    entities: 8_000,
    hotspot_entities: 0,
};
pub const POPULATION_32K: Scenario = Scenario {
    version: 1,
    name: "population-32k",
    seed: Seed(23),
    world_size: 16_384,
    active_extent: 16_384,
    players: 16,
    entities: 32_000,
    hotspot_entities: 0,
};
pub const POPULATION_64K: Scenario = Scenario {
    version: 1,
    name: "population-64k",
    seed: Seed(23),
    world_size: 16_384,
    active_extent: 16_384,
    players: 32,
    entities: 64_000,
    hotspot_entities: 0,
};
pub const POPULATION_128K: Scenario = Scenario {
    version: 1,
    name: "population-128k",
    seed: Seed(23),
    world_size: 16_384,
    active_extent: 16_384,
    players: 64,
    entities: 128_000,
    hotspot_entities: 0,
};
pub const SPARSE_SMALL: Scenario = Scenario {
    version: 1,
    name: "sparse-small",
    seed: Seed(29),
    world_size: 1_024,
    active_extent: 1_024,
    players: 8,
    entities: 8_000,
    hotspot_entities: 0,
};
pub const SPARSE_LARGE: Scenario = Scenario {
    version: 1,
    name: "sparse-large",
    seed: Seed(29),
    world_size: 16_384,
    active_extent: 1_024,
    players: 8,
    entities: 8_000,
    hotspot_entities: 0,
};
pub const BEYOND_TARGET: Scenario = Scenario {
    version: 1,
    name: "beyond-target",
    seed: Seed(31),
    world_size: 16_384,
    active_extent: 16_384,
    players: 128,
    entities: 256_000,
    hotspot_entities: 20_000,
};

impl Scenario {
    pub fn workload_hash(self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.version.to_le_bytes());
        hasher.update(self.name.as_bytes());
        hasher.update(&self.seed.0.to_le_bytes());
        hasher.update(&self.world_size.to_le_bytes());
        hasher.update(&self.active_extent.to_le_bytes());
        hasher.update(&self.players.to_le_bytes());
        hasher.update(&self.entities.to_le_bytes());
        hasher.update(&self.hotspot_entities.to_le_bytes());
        hasher.finalize().to_hex().to_string()
    }
}

pub fn named(name: &str) -> Option<Scenario> {
    match name {
        "smoke" => Some(SMOKE),
        "target-distributed" => Some(TARGET_DISTRIBUTED),
        "target-hotspot" => Some(TARGET_HOTSPOT),
        "network-pressure" => Some(NETWORK_PRESSURE),
        "population-8k" => Some(POPULATION_8K),
        "population-32k" => Some(POPULATION_32K),
        "population-64k" => Some(POPULATION_64K),
        "population-128k" => Some(POPULATION_128K),
        "sparse-small" => Some(SPARSE_SMALL),
        "sparse-large" => Some(SPARSE_LARGE),
        "beyond-target" => Some(BEYOND_TARGET),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn population_series_keeps_workload_shape_comparable() {
        let series = [
            POPULATION_8K,
            POPULATION_32K,
            POPULATION_64K,
            POPULATION_128K,
        ];
        for scenario in series {
            assert_eq!(scenario.version, 1);
            assert_eq!(scenario.seed, Seed(23));
            assert_eq!(scenario.world_size, 16_384);
            assert_eq!(scenario.active_extent, 16_384);
            assert_eq!(scenario.entities, u32::from(scenario.players) * 2_000);
            assert_eq!(named(scenario.name), Some(scenario));
        }
        assert_ne!(series[0].workload_hash(), series[3].workload_hash());
        assert_eq!(named(NETWORK_PRESSURE.name), Some(NETWORK_PRESSURE));
        assert_eq!(
            NETWORK_PRESSURE.hotspot_entities,
            TARGET_HOTSPOT.hotspot_entities
        );
    }
}
