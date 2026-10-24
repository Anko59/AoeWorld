use super::{Client, scene::PreparedScene};
use aoe_rendering::{SceneCamera, SceneResource, SceneUnit};
use std::rc::Rc;

struct RenderedFrame<S> {
    scene: Rc<S>,
    units: Vec<SceneUnit>,
    resources: Vec<(SceneResource, Option<usize>)>,
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
    pub(crate) fn resident_bytes(&self, scene_bytes: impl FnOnce(&S) -> usize) -> usize {
        self.previous.as_ref().map_or(0, |frame| {
            scene_bytes(&frame.scene)
                + frame.units.capacity() * std::mem::size_of::<SceneUnit>()
                + frame.resources.capacity() * std::mem::size_of::<(SceneResource, Option<usize>)>()
        })
    }

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
        art: &aoe_rendering::GameArt,
        units: &[SceneUnit],
        resources: &[SceneResource],
        grid: bool,
        animation: usize,
        camera: SceneCamera,
    ) -> bool {
        let Some(previous) = &self.previous else {
            return true;
        };
        !Rc::ptr_eq(&previous.scene, scene)
            || previous.units.iter().copied().ne(units
                .iter()
                .map(|unit| aoe_rendering::scene_unit_render_key(art, *unit, camera)))
            || previous
                .resources
                .iter()
                .copied()
                .ne(resources.iter().map(|r| {
                    (
                        *r,
                        aoe_rendering::scene_resource_animation_frame(art, *r, animation, camera),
                    )
                }))
            || previous.grid != grid
            || previous.animation != moving_animation(art, units, animation, camera)
    }

    fn record_success(
        &mut self,
        scene: &Rc<S>,
        art: &aoe_rendering::GameArt,
        units: &[SceneUnit],
        resources: &[SceneResource],
        grid: bool,
        animation: usize,
        camera: SceneCamera,
    ) {
        self.previous = Some(RenderedFrame {
            scene: scene.clone(),
            units: units
                .iter()
                .map(|unit| aoe_rendering::scene_unit_render_key(art, *unit, camera))
                .collect(),
            resources: resources
                .iter()
                .map(|r| {
                    (
                        *r,
                        aoe_rendering::scene_resource_animation_frame(art, *r, animation, camera),
                    )
                })
                .collect(),
            grid,
            animation: moving_animation(art, units, animation, camera),
        });
    }
}

fn moving_animation(
    art: &aoe_rendering::GameArt,
    units: &[SceneUnit],
    animation: usize,
    camera: SceneCamera,
) -> Option<usize> {
    units
        .iter()
        .any(|unit| aoe_rendering::scene_unit_animates(art, *unit, camera))
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
    if !client.rendered_frame.needs_render(
        scene,
        &client.art,
        units,
        resources,
        grid,
        animation,
        camera,
    ) {
        return Ok(false);
    }

    let result = {
        let Client {
            renderer, art, fog, ..
        } = client;
        renderer.render_prepared_world_with_fog(
            art,
            &scene.terrain,
            &scene.triangles,
            resources,
            units,
            camera,
            animation,
            grid,
            !fog.disabled(),
            &|point| fog.brightness(point),
        )
    };
    if !result? {
        return Ok(false);
    }
    client.rendered_frame.record_success(
        scene,
        &client.art,
        units,
        resources,
        grid,
        animation,
        camera,
    );
    client.capture.frame = client.capture.frame.saturating_add(1);
    Ok(true)
}

#[path = "render_cache/tests.rs"]
#[cfg(test)]
mod tests;
