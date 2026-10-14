//! One view's two shots as a report row: the scene's pixels compared with the
//! HUD rectangles masked, and the montage row.
use super::{
    Shot,
    bookmarks::Bookmark,
    pixels::{self, Image},
    report::{Observed, Requested, Status, ViewReport},
};
use std::{fs, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn observed(shot: &Shot) -> Option<Observed> {
    let c = &shot.capture;
    Some(Observed {
        camera: c.camera?,
        frame: c
            .history
            .last()
            .map_or(0, |s| u64::try_from(s.frame).unwrap_or(0)),
        settle_ms: c.settle_ms.unwrap_or(0),
        reported_renderer: c.renderer.clone(),
        map_hash: c.map_hash.clone(),
        page_errors: c.page_errors.clone(),
    })
}

/// A view's two shots as a report row, the files it writes and the montage row.
pub(super) fn compare_view(
    mark: &Bookmark,
    before: Option<&Shot>,
    after: Option<&Shot>,
    out: &Path,
) -> Result<(ViewReport, Option<Image>)> {
    let mut view = ViewReport {
        name: mark.name.clone(),
        map: mark.map.clone(),
        renderer: mark.renderer,
        requested: Requested {
            x: mark.x,
            y: mark.y,
            zoom: mark.zoom,
        },
        status: Status::Failed,
        before: before.and_then(observed),
        after: after.and_then(observed),
        scene: None,
        masks: Vec::new(),
        compared_pixels: None,
        changed_pixels: None,
        changed_percent: None,
        masked_changed_pixels: None,
        only_masked_differ: false,
        error: None,
        files: Vec::new(),
    };
    let problems: Vec<String> = [("before", before), ("after", after)]
        .iter()
        .filter_map(|(side, shot)| match shot {
            Some(Shot {
                png: Some(_),
                capture,
            }) if capture.camera.is_some() && capture.scene.is_some() => None,
            Some(Shot { capture, .. }) => Some(format!(
                "{side}: {}",
                capture.error.as_deref().unwrap_or("no screenshot")
            )),
            None => Some(format!("{side}: not captured")),
        })
        .collect();
    let (Some(before), Some(after), true) = (before, after, problems.is_empty()) else {
        view.error = Some(problems.join("; "));
        return Ok((view, None));
    };
    let (Some(bpng), Some(apng)) = (before.png.as_deref(), after.png.as_deref()) else {
        view.error = Some("no screenshot".into());
        return Ok((view, None));
    };
    let mut put = |suffix: &str, bytes: &[u8]| -> Result<()> {
        let file = format!("{}.{suffix}.png", mark.name);
        fs::write(out.join(&file), bytes)?;
        view.files.push(file);
        Ok(())
    };
    put("before", bpng)?;
    put("after", apng)?;
    let (bimg, aimg) = (Image::decode(bpng)?, Image::decode(apng)?);
    let scene = after.capture.scene.unwrap_or_default();
    let mut masks = after.capture.masks.clone();
    for m in &before.capture.masks {
        if !masks.contains(m) {
            masks.push(*m);
        }
    }
    let (Some(bcam), Some(acam)) = (before.capture.camera, after.capture.camera) else {
        return Err("a captured view has no camera".into());
    };
    view.scene = Some(scene);
    view.masks = masks.clone();
    let crop = |i: &Image| i.crop(scene);
    if !bcam.same_view(&acam) {
        view.status = Status::CameraMismatch;
        view.error = Some(format!(
            "before ended at {:.2},{:.2} z{:.3}, after at {:.2},{:.2} z{:.3}",
            bcam.x, bcam.y, bcam.zoom, acam.x, acam.y, acam.zoom
        ));
        let row = pixels::row(
            &mark.name,
            &[
                ("BEFORE", &crop(&bimg)),
                ("AFTER (CAMERA MISMATCH)", &crop(&aimg)),
            ],
        );
        fs::write(
            out.join(format!("{}.montage.png", mark.name)),
            row.encode()?,
        )?;
        view.files.push(format!("{}.montage.png", mark.name));
        return Ok((view, Some(row)));
    }
    let diff = pixels::diff(&bimg, &aimg, scene, &masks)?;
    view.compared_pixels = Some(diff.compared);
    view.changed_pixels = Some(diff.changed);
    view.changed_percent = Some(diff.percent());
    view.masked_changed_pixels = Some(diff.masked_changed);
    view.only_masked_differ = diff.changed == 0 && diff.masked_changed > 0;
    view.status = if diff.changed == 0 {
        Status::Identical
    } else {
        Status::Changed
    };
    put("diff", &diff.image.encode()?)?;
    let caption = if diff.changed == 0 {
        "DIFF: IDENTICAL".to_owned()
    } else {
        format!("DIFF: {} PX ({:.3}%)", diff.changed, diff.percent())
    };
    let row = pixels::row(
        &mark.name,
        &[
            ("BEFORE", &crop(&bimg)),
            ("AFTER", &crop(&aimg)),
            (&caption, &crop(&diff.image)),
        ],
    );
    fs::write(
        out.join(format!("{}.montage.png", mark.name)),
        row.encode()?,
    )?;
    view.files.push(format!("{}.montage.png", mark.name));
    Ok((view, Some(row)))
}
