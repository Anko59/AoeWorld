//! Lossless-reclaim measurement for the frames the proof selects: tight
//! bounding boxes over every drawn plane, identical content, and mirrors.
use aoe_assets::pack::{AtlasPage, FrameRecord};
use std::{collections::HashMap, fs, path::PathBuf};

pub struct Item {
    pub kind: &'static str,
    pub frame: FrameRecord,
}

type Plane = Vec<u8>;

fn load(path: PathBuf) -> Result<Plane, Box<dyn std::error::Error>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(fs::read(path)?));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut data = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut data)?;
    if info.color_type != png::ColorType::Rgba || info.width != 2048 {
        return Err("expected 2048-wide RGBA page".into());
    }
    Ok(data)
}

pub fn report(
    items: &[Item],
    pages: &[AtlasPage],
    dirs: &[PathBuf],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut by_page = HashMap::<u16, Vec<usize>>::new();
    for (index, item) in items.iter().enumerate() {
        by_page.entry(item.frame.page).or_default().push(index);
    }
    let present = by_page.keys().all(|&page| {
        let (record, dir) = (&pages[usize::from(page)], &dirs[usize::from(page)]);
        [&record.color, &record.player, &record.shadow]
            .iter()
            .all(|name| dir.join(name).is_file())
    });
    if !present {
        println!("lossless reclaim: source page images unavailable, skipped");
        return Ok(());
    }
    // Per category: selected, tight, tight with identical frames shared,
    // and additionally with horizontal mirrors shared (all in pixels).
    let mut rows = std::collections::BTreeMap::<&str, [u64; 4]>::new();
    let mut seen = HashMap::<[u8; 32], ()>::new();
    let mut pages_sorted = by_page.keys().copied().collect::<Vec<_>>();
    pages_sorted.sort_unstable();
    // Hashes are global across pages, so process in page order and keep both
    // direct and mirrored digests of every first occurrence.
    let mut mirrored = HashMap::<[u8; 32], ()>::new();
    for page in pages_sorted {
        let record = &pages[usize::from(page)];
        let dir = &dirs[usize::from(page)];
        let owned = [
            load(dir.join(&record.color))?,
            load(dir.join(&record.player))?,
            load(dir.join(&record.shadow))?,
        ];
        let planes = [&owned[0], &owned[1], &owned[2]];
        for &index in &by_page[&page] {
            let item = &items[index];
            let f = &item.frame;
            let (x0, y0) = (usize::from(f.x), usize::from(f.y));
            let (w, h) = (usize::from(f.width), usize::from(f.height));
            let at = |row: usize, col: usize| ((y0 + row) * 2048 + x0 + col) * 4;
            let (mut lo_c, mut hi_c, mut lo_r, mut hi_r) = (w, 0, h, 0);
            for row in 0..h {
                for col in 0..w {
                    if planes.iter().any(|p| p[at(row, col) + 3] != 0) {
                        lo_c = lo_c.min(col);
                        hi_c = hi_c.max(col + 1);
                        lo_r = lo_r.min(row);
                        hi_r = hi_r.max(row + 1);
                    }
                }
            }
            let row_entry = rows.entry(item.kind).or_default();
            row_entry[0] += (w * h) as u64;
            if hi_c == 0 {
                continue; // Fully transparent: one texel would remain.
            }
            let (tw, th) = (hi_c - lo_c, hi_r - lo_r);
            row_entry[1] += (tw * th) as u64;
            let mut direct = blake3::Hasher::new();
            let mut flipped = blake3::Hasher::new();
            direct.update(&[tw as u8, (tw >> 8) as u8, th as u8, (th >> 8) as u8]);
            flipped.update(&[tw as u8, (tw >> 8) as u8, th as u8, (th >> 8) as u8]);
            for plane in planes {
                for row in lo_r..hi_r {
                    for col in lo_c..hi_c {
                        direct.update(&plane[at(row, col)..at(row, col) + 4]);
                        let mirror = lo_c + hi_c - 1 - col;
                        flipped.update(&plane[at(row, mirror)..at(row, mirror) + 4]);
                    }
                }
            }
            let (direct, flipped) = (
                *direct.finalize().as_bytes(),
                *flipped.finalize().as_bytes(),
            );
            let first = seen.insert(direct, ()).is_none();
            if first {
                row_entry[2] += (tw * th) as u64;
            }
            if mirrored.contains_key(&direct) || mirrored.contains_key(&flipped) {
                // Equal to an earlier frame or its mirror image.
            } else if first {
                row_entry[3] += (tw * th) as u64;
            }
            mirrored.insert(direct, ());
        }
    }
    println!(
        "lossless reclaim | selected px | tight px | tight+identical shared px | +mirrors shared px"
    );
    let mut total = [0_u64; 4];
    for (kind, row) in &rows {
        println!(
            "  {kind} | {} | {} | {} | {}",
            row[0], row[1], row[2], row[3]
        );
        for (sum, value) in total.iter_mut().zip(row) {
            *sum += value;
        }
    }
    println!(
        "  total | {} | {} | {} | {}",
        total[0], total[1], total[2], total[3]
    );
    Ok(())
}
