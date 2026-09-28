use aoe_map::{ENVIRONMENT_PAGE_SAMPLES, EnvironmentPageKey, FieldPyramid, MapPackage, PageLayer};

pub(super) fn page_keys(package: &MapPackage) -> Vec<EnvironmentPageKey> {
    let mut keys = Vec::new();
    append_keys(
        &mut keys,
        PageLayer::Elevation,
        &package.environment.elevation,
    );
    if let Some(field) = &package.environment.water {
        append_keys(&mut keys, PageLayer::Water, field);
    }
    if let Some(field) = &package.environment.vegetation {
        append_keys(&mut keys, PageLayer::Vegetation, field);
    }
    if let Some(field) = &package.environment.historical_land_use {
        append_keys(&mut keys, PageLayer::HistoricalLandUse, field);
    }
    if let Some(index) = &package.environment.hydrology_evidence {
        let count = index
            .samples_per_axis
            .div_ceil(u16::from(index.page_samples));
        for layer in [PageLayer::HydrologyEvidence, PageLayer::ModernLandCover] {
            for y in 0..count {
                for x in 0..count {
                    keys.push(EnvironmentPageKey {
                        layer,
                        level: 0,
                        x,
                        y,
                    });
                }
            }
        }
    }
    keys
}

fn append_keys(keys: &mut Vec<EnvironmentPageKey>, layer: PageLayer, field: &FieldPyramid) {
    for (level, metadata) in field.levels.iter().enumerate() {
        let count = metadata
            .samples_per_axis
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        for y in 0..count {
            for x in 0..count {
                keys.push(EnvironmentPageKey {
                    layer,
                    level: level as u8,
                    x,
                    y,
                });
            }
        }
    }
}

#[cfg(test)]
pub(super) fn pyramid_page_count(samples_per_axis: u16) -> usize {
    let mut axis = usize::from(samples_per_axis);
    let mut pages = 0;
    while axis > 0 {
        let side = axis.div_ceil(usize::from(ENVIRONMENT_PAGE_SAMPLES));
        pages += side * side;
        if axis == 1 {
            break;
        }
        axis = axis.div_ceil(2);
    }
    pages
}
