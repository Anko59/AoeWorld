//! Diagnostic top-down frames from the measured state, never game-art copies.
use super::Result;
use aoe_core::{EntityId, TileCoord, WorldPosition};
use aoe_simulation::{GameWorld, Terrain};
use image::{Rgb, RgbImage};
use std::{fs, path::Path};
const W: u32 = 768;
const H: u32 = 320;

pub(super) struct FrameSummary<'a> {
    pub scenario: &'a str,
    pub path: &'a str,
    pub arrived: usize,
}

pub(super) fn frame(
    root: &Path,
    summary: FrameSummary<'_>,
    tick: u64,
    world: &GameWorld,
    ids: &[EntityId],
    starts: &[WorldPosition],
    goal: WorldPosition,
) -> Result<()> {
    if ids.is_empty() || ids.len() != starts.len() || summary.arrived > ids.len() {
        return Err("frame cohort must be nonempty and match its start positions".into());
    }
    let FrameSummary {
        scenario,
        path,
        arrived,
    } = summary;
    let directory = root.join(format!("{scenario}-{path}"));
    fs::create_dir_all(&directory)?;
    let (min_x, max_x, min_y, max_y) = starts.iter().fold(
        (goal.x, goal.x, goal.y, goal.y),
        |(min_x, max_x, min_y, max_y), p| {
            (
                min_x.min(p.x),
                max_x.max(p.x),
                min_y.min(p.y),
                max_y.max(p.y),
            )
        },
    );
    let (min_x, max_x) = (min_x - 25 * 1024, max_x + 25 * 1024);
    let (min_y, max_y) = (min_y - 45 * 1024, max_y + 45 * 1024);
    let project = |p: WorldPosition| {
        (
            ((i64::from(p.x - min_x) * i64::from(W - 1)) / i64::from(max_x - min_x)) as i32,
            25 + ((i64::from(p.y - min_y) * i64::from(H - 50)) / i64::from(max_y - min_y)) as i32,
        )
    };
    let background = directory.join("background.png");
    let mut image = if background.exists() {
        read_png(&background)?
    } else {
        let mut image = RgbImage::from_pixel(W, H, Rgb([34, 48, 35]));
        for y in (25..H - 25).step_by(3) {
            for x in (0..W).step_by(3) {
                let tile = TileCoord::new(
                    (min_x + (i64::from(x) * i64::from(max_x - min_x) / i64::from(W)) as i32)
                        / 1024,
                    (min_y
                        + (i64::from(y - 25) * i64::from(max_y - min_y) / i64::from(H - 50))
                            as i32)
                        / 1024,
                );
                let color = match world.terrain() {
                    Terrain::Map { generator, overlay } => {
                        if overlay.starting_nodes_in(tile, tile).any(|node| {
                            node.kind == aoe_map::ResourceKind::Wood
                                && overlay.resource_available(node)
                        }) {
                            [14, 72, 29]
                        } else {
                            match generator
                                .tile_and_node_with_cancel(tile, &|| false)
                                .ok()
                                .flatten()
                            {
                                Some((t, _)) if t.water != aoe_map::WaterKind::None => {
                                    [38, 98, 151]
                                }
                                Some((_, Some(_))) => [14, 72, 29],
                                Some((t, _)) if !t.passable => [88, 83, 72],
                                _ => [58, 78, 45],
                            }
                        }
                    }
                    _ => [58, 78, 45],
                };
                dot(&mut image, x as i32, y as i32, 2, color);
            }
        }
        save_png(&image, &background)?;
        image
    };
    for b in world.buildings() {
        let p = WorldPosition::from_tile_center(b.footprint.min)?;
        let (x, y) = project(p);
        dot(&mut image, x, y, 3, [174, 161, 119]);
    }
    let (x, y) = project(goal);
    dot(&mut image, x, y, 5, [240, 216, 63]);
    for unit in world.units().filter(|u| u.hp > 0) {
        let (x, y) = project(unit.position);
        let color = if ids.contains(&unit.id) {
            [80, 188, 255]
        } else if unit.player.0 == 0 {
            [194, 198, 206]
        } else {
            [242, 90, 75]
        };
        dot(&mut image, x, y, 2, color);
        if ids.contains(&unit.id)
            && let Some(slot) = world.formation_slot(unit.id)
        {
            let (x, y) = project(slot);
            dot(&mut image, x, y, 1, [215, 160, 240]);
        }
    }
    // Render the same first-arrival count as the report, including actors that
    // subsequently leave their slot or disappear. Current proximity is not it.
    for x in 0..(W * arrived as u32 / ids.len() as u32) {
        for y in H - 12..H {
            image.put_pixel(x, y, Rgb([80, 188, 255]));
        }
    }
    let filename = directory.join(format!("frame-{tick:06}.png"));
    save_png(&image, &filename)?;
    fs::write(
        directory.join("caption.txt"),
        format!(
            "{scenario} / {path}\nBlue: measured cohort; red: enemies; purple: intended slots; yellow: destination.\nSample every 100 simulation ticks plus the final tick; diagnostic projection, not a rendered gameplay qualification.\n"
        ),
    )?;
    Ok(())
}
fn dot(image: &mut RgbImage, x: i32, y: i32, r: i32, color: [u8; 3]) {
    for dy in -r..=r {
        for dx in -r..=r {
            let (x, y) = (x + dx, y + dy);
            if x >= 0 && y >= 0 && x < W as i32 && y < H as i32 {
                image.put_pixel(x as u32, y as u32, Rgb(color));
            }
        }
    }
}

fn save_png(image: &RgbImage, path: &Path) -> Result<()> {
    let file = std::io::BufWriter::new(fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, W, H);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(image.as_raw())?;
    Ok(())
}
fn read_png(path: &Path) -> Result<RgbImage> {
    let mut reader = png::Decoder::new(fs::File::open(path)?).read_info()?;
    let info = reader.info();
    if info.width != W
        || info.height != H
        || info.color_type != png::ColorType::Rgb
        || info.bit_depth != png::BitDepth::Eight
    {
        return Err("unexpected diagnostic background PNG format".into());
    }
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut pixels)?;
    pixels.truncate(frame.buffer_size());
    RgbImage::from_raw(W, H, pixels).ok_or_else(|| "invalid diagnostic PNG buffer".into())
}
