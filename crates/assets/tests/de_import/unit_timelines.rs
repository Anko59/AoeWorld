use super::*;

#[test]
fn real_all_unit_roster_projectiles_and_siege_timing_follow_dat_records() {
    let Some(root) = std::env::var_os("AOE_DE_DIR") else {
        return;
    };
    use aoe_assets::catalog::units::{Pose, SPRITES, projectile_pose, release_frame};
    let root = Path::new(&root);
    let dat = aoe_assets::dat::de_survey::inflate(
        &fs::read(root.join("resources/_common/dat/empires2_x2_p1.dat")).unwrap(),
    )
    .unwrap();
    let shooters = [
        (539, 512),
        (21, 372),
        (442, 373),
        (1103, 676),
        (529, 676),
        (532, 676),
        (420, 374),
        (691, 374),
        (2633, 1932),
        (2626, 2636),
        (2627, 2636),
        (2628, 2636),
        (39, 477),
        (474, 478),
        (279, 367),
        (542, 627),
        (5, 380),
        (36, 368),
        (280, 656),
        (550, 656),
        (588, 656),
        (42, 371),
        (331, -1),
    ];
    let mut ids: Vec<_> = shooters
        .iter()
        .flat_map(|&(unit, projectile)| [unit, projectile])
        .filter(|&id| id >= 0)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let units = aoe_assets::dat::de_units::find(&dat, &ids).unwrap();
    let catalog = de::unit_catalog::resolve(root).unwrap();
    for (pose, graphic, frames) in [
        (Pose::FireGalleyWalk, units[&1103].walking, 30),
        (Pose::CatapultGalleonAttack, units[&2633].attack, 45),
    ] {
        let chosen = catalog
            .iter()
            .find(|entry| entry.pose == pose as usize)
            .unwrap();
        assert_eq!(
            (chosen.graphic, chosen.frames),
            (graphic, frames),
            "{pose:?}"
        );
    }
    for (unit, projectile) in shooters {
        assert_eq!(
            units[&unit].projectile, projectile,
            "DAT projectile of {unit}"
        );
        if projectile < 0 {
            continue;
        }
        let pose = projectile_pose(projectile as u16).expect("mapped DAT projectile");
        let chosen = catalog
            .iter()
            .find(|entry| entry.pose == pose as usize)
            .unwrap();
        assert_eq!(chosen.graphic, units[&projectile].standing, "{unit}");
        assert_eq!(
            aoe_assets::catalog::units::projectile_arc(projectile as u16) as f32,
            units[&projectile].projectile_arc.unwrap(),
            "DAT trajectory for {unit}",
        );
    }
    for (unit, pose, frames, seconds) in [
        (280, Pose::MangonelAttack, 60, 1.0),
        (550, Pose::OnagerAttack, 60, 1.0),
        (588, Pose::SiegeOnagerAttack, 60, 0.7),
        (279, Pose::ScorpionAttack, 60, 0.8),
        (542, Pose::HeavyScorpionAttack, 30, 0.8),
    ] {
        let actual = catalog
            .iter()
            .find(|entry| entry.pose == pose as usize)
            .unwrap();
        let source = SPRITES[pose as usize];
        assert_eq!(actual.graphic, units[&unit].attack);
        assert_eq!(release_frame(pose), u8::try_from(units[&unit].release).ok());
        assert_eq!(source.stride, frames);
        assert!((f64::from(frames) * source.frame_millis() / 1000.0 - seconds).abs() < 0.00001);
    }
    for (unit, transforms_to) in [(42, 331), (331, 42)] {
        let record = &units[&unit];
        let building = record.building.as_ref().unwrap();
        assert_eq!(building.transform_unit, transforms_to);
        assert_eq!(building.train_seconds, Some(50));
        assert_eq!(record.work_rate, 4.5);
        assert_eq!(record.speed, if unit == 42 { 0.0 } else { 0.8 });
    }
}

#[test]
fn real_all_unit_trebuchet_clears_pixels_from_earlier_arm_positions() {
    let Some(root) = std::env::var_os("AOE_DE_DIR") else {
        return;
    };
    let sprite = Path::new(&root)
        .join("resources/_common/drs/graphics/u_sie_unpacked_trebuchet_attackA_x1.sld");
    let mut frames = aoe_assets::sld::Stream::new(fs::read(sprite).unwrap()).unwrap();
    let layer = frames.frame(24).unwrap().main.as_ref().unwrap();
    let alpha = |x, y| {
        let column = usize::from(x - layer.x);
        let row = usize::from(y - layer.y);
        layer.pixels[row * usize::from(layer.width) + column][3]
    };
    assert_eq!(alpha(306, 73), 0, "earlier arm position");
    assert_eq!(alpha(305, 285), 255, "carriage at the current position");
}

#[test]
fn real_all_unit_secondary_projectiles_follow_the_dat_volley() {
    let Some(root) = std::env::var_os("AOE_DE_DIR") else {
        return;
    };
    let root = Path::new(&root);
    let dat = aoe_assets::dat::de_survey::inflate(
        &fs::read(root.join("resources/_common/dat/empires2_x2_p1.dat")).unwrap(),
    )
    .unwrap();
    let units = aoe_assets::dat::de_units::find(&dat, &[369, 656]).unwrap();
    let catalog = de::unit_catalog::resolve(root).unwrap();
    let flight = |id| {
        let pose = aoe_assets::catalog::units::projectile_pose(id).unwrap();
        catalog
            .iter()
            .find(|entry| entry.pose == pose as usize)
            .unwrap()
    };
    assert_ne!(flight(369).graphic, flight(656).graphic);
    assert_eq!(flight(369).graphic, units[&369].standing);
    assert_eq!(
        aoe_assets::catalog::units::projectile_arc(369) as f32,
        units[&369].projectile_arc.unwrap()
    );
    for (id, count, width) in [(280, 6, 1.0), (550, 8, 1.25), (588, 10, 1.5)] {
        let unit =
            serde_json::to_value(aoe_assets::dat::de_survey::soldier(&dat, id).unwrap()).unwrap();
        assert_eq!(unit["total_projectiles"], f64::from(count));
        assert_eq!(unit["max_total_projectiles"], count);
        assert_eq!(
            unit["projectile_spawning_area"],
            serde_json::json!([width, width, 99.0])
        );
        assert_eq!(unit["secondary_projectile"], 369);
    }
    for id in [2626, 2627, 2628] {
        let unit =
            serde_json::to_value(aoe_assets::dat::de_survey::soldier(&dat, id).unwrap()).unwrap();
        assert_eq!(unit["projectile"], 2636);
        assert_eq!(unit["total_projectiles"], 3.0);
        assert_eq!(unit["max_total_projectiles"], 3);
        assert_eq!(unit["secondary_projectile"], -1);
    }
}

#[test]
fn real_all_unit_timelines_match_de_dat_and_complete_native_sld_cells() {
    let Some(root) = std::env::var_os("AOE_DE_DIR") else {
        return;
    };
    let root = Path::new(&root);
    let dat = aoe_assets::dat::de_survey::inflate(
        &fs::read(root.join("resources/_common/dat/empires2_x2_p1.dat")).unwrap(),
    )
    .unwrap();
    let units = aoe_assets::dat::de_units::find(&dat, &[4, 24, 7, 6, 5, 42]).unwrap();
    use aoe_assets::catalog::units::{Pose, release_frame};
    for (id, pose) in [
        (4, Pose::ArcherAttack),
        (24, Pose::CrossbowmanAttack),
        (7, Pose::SkirmisherAttack),
        (6, Pose::EliteSkirmisherAttack),
        (5, Pose::HandCannoneerAttack),
        (42, Pose::TrebuchetUnpackedAttack),
    ] {
        assert_eq!(
            u8::try_from(units[&id].release).ok(),
            release_frame(pose),
            "DAT release frame for {id}"
        );
    }
    let selected = de::unit_catalog::resolve(root).unwrap();
    assert_eq!(
        selected.len() + 4,
        aoe_assets::catalog::units::SPRITES.len()
    );
    let mut seen = std::collections::BTreeSet::new();
    for g in selected {
        let source = aoe_assets::catalog::units::SPRITES[g.pose];
        assert_eq!(source.id, g.sprite);
        assert_eq!(source.rows(), usize::from(g.angles));
        assert_eq!(
            u16::from(source.stride),
            if g.terminal { 1 } else { g.frames }
        );
        assert_eq!(
            source.frame_millis(),
            if g.terminal {
                0.0
            } else {
                f64::from(g.seconds) * 1000.0
            }
        );
        for part in g.parts {
            let a = part.animation;
            if !seen.insert(a.file.clone()) {
                continue;
            }
            let mut frames = aoe_assets::sld::Stream::new(
                fs::read(
                    root.join("resources/_common/drs/graphics")
                        .join(format!("{}.sld", a.file)),
                )
                .unwrap(),
            )
            .unwrap();
            assert!(
                frames.len() >= usize::from(a.frames) * usize::from(a.angles),
                "{}",
                a.file
            );
            let mut has_main = false;
            let mut has_player = false;
            let mut has_shadow = false;
            let mut varied_rows = vec![false; usize::from(a.angles)];
            let mut previous = None;
            for cell in 0..frames.len() {
                let frame = frames
                    .frame(cell)
                    .unwrap_or_else(|error| panic!("{} frame {cell}: {error}", a.file));
                has_main |= frame.main.is_some();
                has_player |= frame.player.is_some();
                has_shadow |= frame.shadow.is_some();
                if (130..135).contains(&g.pose) {
                    let stride = usize::from(a.frames);
                    if let Some(varied) = varied_rows.get_mut(cell / stride)
                        && cell % stride != 0
                    {
                        *varied |= previous.as_ref().is_some_and(|last| last != frame);
                    }
                    previous = Some(frame.clone());
                }
            }
            if a.file.ends_with("_shadow_x1") {
                assert!(has_shadow, "{}", a.file);
            } else {
                assert!(has_main, "{}", a.file);
            }
            if (130..135).contains(&g.pose) {
                assert!(has_player, "{}", a.file);
                for (row, varied) in varied_rows.into_iter().enumerate() {
                    assert!(varied, "{} row {row}", a.file);
                }
            }
        }
    }
    assert!(seen.len() > 140);
}
