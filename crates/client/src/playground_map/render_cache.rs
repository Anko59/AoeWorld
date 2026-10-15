use super::{Client, scene::PreparedScene};
use aoe_rendering::{SceneCamera, SceneResource, SceneUnit};
use std::rc::Rc;

struct RenderedFrame<S> {
    scene: Rc<S>,
    units: Vec<SceneUnit>,
    resources: Vec<SceneResource>,
    grid: bool,
    animation: Option<usize>,
}

/// Remembers only the last successfully rendered, currently visible frame.
pub(crate) struct RenderFrameCache<S> {
    previous: Option<RenderedFrame<S>>,
}

impl<S> Default for RenderFrameCache<S> {
    fn default() -> Self {
        Self { previous: None }
    }
}

impl<S> RenderFrameCache<S> {
    pub(crate) fn position(&self, id: aoe_core::EntityId) -> Option<[f64; 2]> {
        // Scene units preserve the authoritative BTreeMap's entity-ID order.
        let units = &self.previous.as_ref()?.units;
        let index = units.binary_search_by_key(&id, |unit| unit.id).ok()?;
        Some(units[index].position)
    }

    pub(crate) fn clear(&mut self) {
        self.previous = None;
    }

    fn needs_render(
        &self,
        scene: &Rc<S>,
        units: &[SceneUnit],
        resources: &[SceneResource],
        grid: bool,
        animation: usize,
    ) -> bool {
        let Some(previous) = &self.previous else {
            return true;
        };
        !Rc::ptr_eq(&previous.scene, scene)
            || previous.units.as_slice() != units
            || previous.resources.as_slice() != resources
            || previous.grid != grid
            || previous.animation != moving_animation(units, animation)
    }

    fn record_success(
        &mut self,
        scene: &Rc<S>,
        units: &[SceneUnit],
        resources: &[SceneResource],
        grid: bool,
        animation: usize,
    ) {
        self.previous = Some(RenderedFrame {
            scene: scene.clone(),
            units: units.to_vec(),
            resources: resources.to_vec(),
            grid,
            animation: moving_animation(units, animation),
        });
    }
}

fn moving_animation(units: &[SceneUnit], animation: usize) -> Option<usize> {
    units
        .iter()
        .any(|unit| unit.moving || unit.kind == aoe_rendering::SceneUnitKind::VillagerWorking)
        .then_some(animation)
}

pub(in super::super) fn render_if_changed(
    client: &mut Client,
    scene: &Rc<PreparedScene>,
    units: &[SceneUnit],
    resources: &[SceneResource],
    camera: SceneCamera,
    animation: usize,
    grid: bool,
) -> Result<bool, String> {
    if !client
        .rendered_frame
        .needs_render(scene, units, resources, grid, animation)
    {
        return Ok(false);
    }

    let result = {
        let Client { renderer, art, .. } = client;
        renderer.render_prepared_world(
            art,
            &scene.terrain,
            &scene.triangles,
            resources,
            units,
            camera,
            animation,
            grid,
        )
    };
    if !result? {
        return Ok(false);
    }
    client
        .rendered_frame
        .record_success(scene, units, resources, grid, animation);
    client.capture.frame = client.capture.frame.saturating_add(1);
    Ok(true)
}

#[path = "render_cache/tests.rs"]
#[cfg(test)]
mod tests;
