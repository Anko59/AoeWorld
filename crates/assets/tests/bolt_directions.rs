//! The galley bolt's drawn shaft must point where the shot flies. Cell angles
//! are measured from the decoded DE frames, not assumed from their order.
use aoe_assets::catalog::units::{Pose, SPRITES, projectile_facing};
use std::{fs, path::Path};

/// Screen direction of the bolt's shaft in each native cell, degrees clockwise
/// from screen east (y grows down), from its fletching (blue) to its head.
fn measured_cell_angles(root: &Path) -> Vec<f64> {
    let path = root.join("resources/_common/drs/graphics/p_bolt_x1.sld");
    let frames = aoe_assets::sld::parse(&fs::read(path).unwrap()).unwrap();
    frames
        .iter()
        .map(|frame| {
            let mut tail = ([0.0; 2], 0.0);
            let mut head = ([0.0; 2], 0.0);
            let add = |side: &mut ([f64; 2], f64),
                       layer: &aoe_assets::sld::Layer,
                       keep: &dyn Fn(&[u8; 4]) -> bool| {
                for (at, p) in layer.pixels.iter().enumerate() {
                    if p[3] > 0 && keep(p) {
                        side.0[0] += f64::from(layer.x) + (at % usize::from(layer.width)) as f64;
                        side.0[1] += f64::from(layer.y) + (at / usize::from(layer.width)) as f64;
                        side.1 += 1.0;
                    }
                }
            };
            // The player-coloured fletching is the tail; the grey iron head the tip.
            if let Some(player) = &frame.player {
                add(&mut tail, player, &|_| true);
            }
            let main = frame.main.as_ref().unwrap();
            add(&mut head, main, &|p| {
                i32::from(p[0]).abs_diff(i32::from(p[2])) < 20
            });
            assert!(tail.1 > 0.0 && head.1 > 0.0, "fletching and head found");
            let (dx, dy) = (
                head.0[0] / head.1 - tail.0[0] / tail.1,
                head.0[1] / head.1 - tail.0[1] / tail.1,
            );
            // The shaft's axis is the principal axis of every opaque pixel; the
            // fletching-to-head vector only chooses which end is the tip.
            let (mut n, mut mx, mut my) = (0.0, 0.0, 0.0);
            let mut points = Vec::new();
            for layer in [frame.main.as_ref(), frame.player.as_ref()]
                .into_iter()
                .flatten()
            {
                for (at, p) in layer.pixels.iter().enumerate() {
                    if p[3] > 0 {
                        let x = f64::from(layer.x) + (at % usize::from(layer.width)) as f64;
                        let y = f64::from(layer.y) + (at / usize::from(layer.width)) as f64;
                        points.push((x, y));
                        (n, mx, my) = (n + 1.0, mx + x, my + y);
                    }
                }
            }
            let (mx, my) = (mx / n, my / n);
            let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
            for (x, y) in points {
                sxx += (x - mx) * (x - mx);
                syy += (y - my) * (y - my);
                sxy += (x - mx) * (y - my);
            }
            let axis = (0.5 * (2.0 * sxy).atan2(sxx - syy)).to_degrees();
            let forward = (axis.to_radians().cos() * dx + axis.to_radians().sin() * dy) >= 0.0;
            (if forward { axis } else { axis + 180.0 }).rem_euclid(360.0)
        })
        .collect()
}

fn gap(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

#[test]
fn real_galley_bolt_shaft_follows_each_flight_heading() {
    let Some(root) = std::env::var_os("AOE_DE_DIR") else {
        return;
    };
    let angles = measured_cell_angles(Path::new(&root));
    assert_eq!(angles.len(), 32);
    let source = SPRITES[Pose::Bolt as usize];
    // The eight tile steps and the screen direction they project to (2:1).
    let mut report = String::new();
    for (dx, dy) in [
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
    ] {
        let (east, south) = (f64::from(dx - dy), f64::from(dx + dy) / 2.0);
        let wanted = south.atan2(east).to_degrees().rem_euclid(360.0);
        let packed = projectile_facing(f64::from(dx - dy), f64::from(dx + dy));
        let facing = usize::from(packed & 7);
        let row = [0, 1, 2, 3, 4, 3, 2, 1][facing];
        let mirrored = (1..=3).contains(&facing);
        let cell = source.source_frame(row, usize::from(packed >> 3));
        let drawn = if mirrored {
            (180.0 - angles[cell]).rem_euclid(360.0)
        } else {
            angles[cell]
        };
        report += &format!(
            "tile({dx},{dy}) want {wanted:.0} cell {cell} mirrored {mirrored} drawn {drawn:.0}\n"
        );
        assert!(
            gap(drawn, wanted) <= 12.0,
            "{report}cells {:?}",
            angles.iter().map(|a| a.round()).collect::<Vec<_>>()
        );
    }
}
