//! Build immutable semantic art before starting asynchronous page uploads.
use super::*;
use std::ops::Range;
pub(super) struct Input<'a> {
    pub manifest: &'a Manifest,
    pub selected: &'a [usize],
    pub records: &'a [GameFrame],
    pub ranges: &'a RoleRanges,
    pub actor_ranges: &'a [Option<Range<usize>>],
    pub rally_range: Option<Range<usize>>,
    pub native_farm: Option<(Range<usize>, TerrainTopology)>,
    pub terrain_topology: [Option<TerrainTopology>; 12],
    pub depth_topology: [Option<TerrainTopology>; 4],
    pub residents: usize,
}
#[inline(never)]
pub(super) fn resolve(input: Input<'_>) -> Result<GameArt, JsValue> {
    let Input {
        manifest,
        selected,
        records,
        ranges,
        actor_ranges,
        rally_range,
        native_farm,
        terrain_topology,
        depth_topology,
        residents,
    } = input;
    let group = |role| group_frames(&ranges, &records, role);
    let mut terrain: [Vec<GameFrame>; 12] = Default::default();
    actors::fill_groups(&mut terrain, &TERRAIN_ROLES, &ranges, &records);
    let mut village: [Vec<GameFrame>; 9] = Default::default();
    actors::fill_groups(
        &mut village,
        &[
            AssetRole::VillagerStanding,
            AssetRole::VillagerWalking,
            AssetRole::VillagerChopping,
            AssetRole::VillagerCarryingWood,
            AssetRole::VillagerForaging,
            AssetRole::VillagerCarryingFood,
            AssetRole::VillagerCarryingGold,
            AssetRole::VillagerCarryingStone,
            AssetRole::VillagerMiningStone,
        ],
        &ranges,
        &records,
    );
    let mut environment = Vec::new();
    let mut resolved_buildings = Vec::new();
    for b in buildings::resolve(&manifest, &selected, &records) {
        if aoe_assets::catalog::ENVIRONMENT_UNIT_IDS.contains(&b.unit_id) {
            environment.push(b);
        } else {
            resolved_buildings.push(b);
        }
    }
    let buildings = resolved_buildings;
    let foundation = if let Some((r, t)) = native_farm {
        (records[r].to_vec(), Some(t))
    } else {
        buildings::foundation(&manifest, &selected, &records)?
    };

    let native_cliffs = environment
        .iter()
        .find(|b| b.unit_id == 264)
        .and_then(|b| b.parts.first())
        .map(|p| {
            if p.animation.is_empty() {
                vec![p.frame]
            } else {
                p.animation.clone()
            }
        });
    Ok(GameArt {
        resident: vec![None; residents],
        building_clocks: Vec::new(),
        environment,
        actor_rows: crate::building_ui::content::sprites()
            .iter()
            .map(|s| s.rows())
            .collect(),
        actors: actors::resolve(&manifest, &selected, &records, &actor_ranges),
        fishing: Default::default(),
        walking: group(AssetRole::CavalryWalking),
        standing: group(AssetRole::CavalryStanding),
        village,
        compositions: buildings,
        buildings: std::array::from_fn(|_| Vec::new()),
        foundation,
        grass: terrain[0].clone(),
        terrain,
        terrain_topology,
        water_depth: std::array::from_fn(|slot| {
            (group(WATER_DEPTH_ROLES[slot]), depth_topology[slot])
        }),
        resources: [
            group(AssetRole::ForageBush),
            group(AssetRole::WoodTree),
            group(AssetRole::GoldDeposit),
            group(AssetRole::StoneDeposit),
        ],
        tree_shadows: group(AssetRole::WoodTreeShadow),
        tree_families: [group(AssetRole::TreeConifer), group(AssetRole::TreePalm)],
        scenery: aoe_rendering::SceneryArt {
            rally_flags: building_sprites::rally_frames(
                &records,
                rally_range,
                group(AssetRole::RallyFlag),
            ),
            cliff_faces: native_cliffs.unwrap_or_else(|| group(AssetRole::CliffFace)),
            rocks: group(AssetRole::RockPile),
            shrubs: group(AssetRole::Shrub),
            stumps: group(AssetRole::DeadwoodStump),
            snags: group(AssetRole::DeadwoodTree),
            logs: group(AssetRole::DeadwoodLog),
        },
    })
}
