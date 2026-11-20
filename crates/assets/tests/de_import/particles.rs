//! Synthetic emitter records exercise the same importer paths as native effects.
use aoe_assets::{
    catalog::units::{Pose, SPRITES},
    de::unit_catalog::{Action, BINDINGS},
};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Default)]
pub(super) struct Particles {
    names: BTreeMap<i16, &'static str>,
    deltas: BTreeMap<i16, Vec<i16>>,
}

impl Particles {
    pub(super) fn name(&self, graphic: i16) -> &str {
        self.names.get(&graphic).copied().unwrap_or("")
    }

    pub(super) fn deltas(&self, graphic: i16) -> &[i16] {
        self.deltas.get(&graphic).map_or(&[], Vec::as_slice)
    }
}

pub(super) fn write(install: &Path) -> Particles {
    let path = install.join("resources/_common/particles");
    let atlases = path.join("textures/atlases");
    fs::create_dir_all(&atlases).unwrap();
    fs::write(atlases.join("fixture.dds"), super::super::dds()).unwrap();
    let cell = json!({
        "frame": { "x": 0, "y": 0, "w": 4, "h": 4 },
        "rotated": false,
        "spriteSourceSize": { "x": 0, "y": 0, "w": 4, "h": 4 },
        "sourceSize": { "w": 4, "h": 4 },
        "pivot": { "x": 0.5, "y": 0.5 },
    });
    fs::write(
        atlases.join("fixture.json"),
        json!({ "frames": vec![cell; 1024] }).to_string(),
    )
    .unwrap();
    let mut particles = Particles::default();
    for binding in BINDINGS {
        let parent_action: fn(&Action) -> bool;
        let name = match binding.action {
            Action::AttackEffect(name) => {
                parent_action = |action| matches!(action, Action::Attack);
                name
            }
            Action::DeathEffect(name) => {
                parent_action = |action| matches!(action, Action::Death);
                name
            }
            Action::Projectile if binding.pose == Pose::FireStream => {
                parent_action = |_| false;
                "flamethrower_flame"
            }
            _ => continue,
        };
        let source = SPRITES[binding.pose as usize];
        let graphic = (source.id - 200_000) as i16;
        particles.names.insert(graphic, name);
        if let Some(parent) = BINDINGS
            .iter()
            .find(|b| b.unit == binding.unit && parent_action(&b.action))
        {
            let parent = (SPRITES[parent.pose as usize].id - 200_000) as i16;
            particles.deltas.entry(parent).or_default().push(graphic);
        }
        fs::write(
            path.join(format!("{name}.json")),
            json!({
                "AtlasFile": "textures/atlases/fixture.dds",
                "ImageCount": source.selected_frames(),
                "ImageAngles": source.rows(),
                "Duration": source.frame_millis() * f64::from(source.stride) / 1000.0,
                "Scale": 1.0,
            })
            .to_string(),
        )
        .unwrap();
    }
    particles
}
