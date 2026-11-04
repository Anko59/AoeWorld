//! Shared synthetic census workload; excluded from normal server builds.
use super::*;
use aoe_core::{Seed, TileCoord, TileRect, WorldConfig};
use aoe_simulation::WorldLimits;

pub(super) fn views(connected: usize) -> Vec<View> {
    (0..connected)
        .map(|id| View {
            session_id: id as u64,
            subscription: Subscription {
                revision: 1,
                region: TileRect::from_xywh(0, 0, 32, 32),
            },
            vision: None,
            fog_disabled: Some(false),
            viewer: PlayerId(id as u16),
            primary: EntityId(id as u32),
            ai: false,
            rebuild: false,
        })
        .collect()
}

impl super::super::GameplayService {
    /// 100k units, 1,000 registered economy players, with private fogged reports.
    /// Fixture construction is outside instruction measurement.
    pub fn census_benchmark_world() -> Result<GameWorld, Box<dyn std::error::Error>> {
        let mut world =
            GameWorld::with_population(WorldConfig::new(4096, 4096, Seed(7))?, 100_000, 0, 1000)?;
        world.set_limits(WorldLimits::for_match(1000));
        let mut locations =
            (0..10_000).map(|n| TileCoord::new(10 + (n % 300) * 12, 10 + (n / 300) * 12));
        for id in 0..1000u16 {
            let placed =
                locations.find_map(|tile| world.place_town_center(PlayerId(id), tile).ok());
            if placed.is_none() {
                return Err("fixture needs 1,000 valid town centers".into());
            }
        }
        world.set_fog_disabled(false);
        assert_eq!(world.players().count(), 1000);
        assert_eq!(world.unit_count(), 100_000);
        Ok(world)
    }

    /// Actual serial viewer-state construction, excluding simulation and transport.
    /// The instruction benchmark repeats this twenty times (one second at 20 Hz).
    pub fn benchmark_player_reports(world: &GameWorld, connected: usize) -> usize {
        let states = viewer_states(world, &views(connected), &BTreeMap::new());
        std::hint::black_box(&states);
        states.values().map(|state| state.players.len()).sum()
    }
}
