//! Emit bounded native bridge pieces through the shared sprite backends.
use super::*;
use aoe_protocol::GameplayBuildingState;
use aoe_rendering::{SceneUnit, SceneUnitKind, SceneUnitVisual};

#[inline(never)]
pub(super) fn append(
    client: &Client,
    scene: &PreparedScene,
    building: &GameplayBuildingState,
    units: &mut Vec<SceneUnit>,
) {
    let rect = building.footprint;
    let horizontal = rect.height() == 1;
    let length = rect.width().max(rect.height()).min(42);
    let first = [f64::from(rect.min.x) + 0.5, f64::from(rect.min.y) + 0.5];
    let last = [f64::from(rect.max.x) - 0.5, f64::from(rect.max.y) - 0.5];
    // Cull a whole span before allocating its pieces. Native bridge sprites
    // stay within the atlas page; this conservative margin covers their anchors.
    let a = super::super::screen_position(client, first);
    let b = super::super::screen_position(client, last);
    let margin = 512.0 * client.camera.zoom;
    if a.x.max(b.x) < -margin
        || a.x.min(b.x) > client.camera.viewport[0] + margin
        || a.y.max(b.y) < -margin
        || a.y.min(b.y) > client.camera.viewport[1] + margin
    {
        return;
    }
    let camera = SceneCamera {
        center: client.camera.center,
        zoom: client.camera.zoom,
        viewport: client.camera.viewport,
        focus_elevation_meters: client.camera.focus_elevation_meters,
    };
    let insert = units.partition_point(|other| other.id <= building.id);
    let before = units.len();
    for offset in 0..length {
        let position = [
            first[0] + if horizontal { f64::from(offset) } else { 0.0 },
            first[1] + if horizontal { 0.0 } else { f64::from(offset) },
        ];
        let kind = if building.progress_permille < 1000 {
            SceneUnitKind::building(9)
        } else {
            SceneUnitKind::bridge(
                horizontal,
                if offset == 0 {
                    0
                } else if offset + 1 == length {
                    2
                } else {
                    1
                },
            )
        };
        let unit = SceneUnit {
            kind,
            id: building.id,
            position,
            visual: SceneUnitVisual::new(false)
                .with_player(
                    Some(building.player),
                    super::super::super::village::owner(client),
                    client.team_colour,
                    false,
                )
                .with_selected(client.selected.has_building(building.id)),
            facing: (u32::from(building.progress_permille) * 3 / 1000).min(2) as u8,
            elevation_meters: scene
                .height(position)
                .unwrap_or_else(|| elevation_at_world(client, position)),
        };
        if aoe_rendering::scene_building_visible(&client.art, unit, camera, 0) {
            units.push(unit);
        }
    }
    // Move the tail once for the whole span, preserving canonical entity order.
    let count = units.len() - before;
    units[insert..].rotate_right(count);
}
