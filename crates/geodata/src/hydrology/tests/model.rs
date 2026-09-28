use super::*;
use aoe_map::{HydrologyEvidenceMethod, WaterCorrectionProjection, WaterCorrectionVertex};

fn pages_for_axis(axis: u16, kinds: &[HydrologyKind]) -> Vec<HydrologyPage> {
    let page_count = axis.div_ceil(PAGE);
    (0..page_count)
        .flat_map(|y| (0..page_count).map(move |x| (x, y)))
        .map(|(x, y)| {
            let width = (axis - x * PAGE).min(PAGE) as u8;
            let height = (axis - y * PAGE).min(PAGE) as u8;
            let mut page_kinds = Vec::new();
            let mut methods = Vec::new();
            for local_y in 0..u16::from(height) {
                for local_x in 0..u16::from(width) {
                    let global = usize::from(y * PAGE + local_y) * usize::from(axis)
                        + usize::from(x * PAGE + local_x);
                    let kind = kinds[global];
                    page_kinds.push(kind as u8);
                    methods.push(match kind {
                        HydrologyKind::Land | HydrologyKind::Shallow => {
                            HydrologyEvidenceMethod::WorldCoverClass
                        }
                        HydrologyKind::Ocean => HydrologyEvidenceMethod::OverviewOcean,
                        HydrologyKind::Lake
                        | HydrologyKind::Reservoir
                        | HydrologyKind::RegulatedLake
                        | HydrologyKind::UnknownWater => HydrologyEvidenceMethod::HydroLakesExtent,
                        HydrologyKind::River => {
                            HydrologyEvidenceMethod::HydroRiversBufferedCorridor
                        }
                        HydrologyKind::NoEvidence => HydrologyEvidenceMethod::None,
                    } as u8);
                }
            }
            HydrologyPage {
                level: 0,
                x,
                y,
                width,
                height,
                kind: page_kinds,
                method: methods,
                water_model: None,
            }
        })
        .collect()
}

fn elevation_pages(axis: u16, value: i32) -> Vec<ElevationPage> {
    let page_count = axis.div_ceil(PAGE);
    (0..page_count)
        .flat_map(|y| (0..page_count).map(move |x| (x, y)))
        .map(|(x, y)| {
            let width = (axis - x * PAGE).min(PAGE) as u8;
            let height = (axis - y * PAGE).min(PAGE) as u8;
            ElevationPage {
                level: 0,
                x,
                y,
                width,
                height,
                geographic_height_centimeters: vec![
                    value;
                    usize::from(width) * usize::from(height)
                ],
            }
        })
        .collect()
}

fn modeled_cell(pages: &[HydrologyPage], axis: u16, x: u16, y: u16) -> (u8, Option<i32>, u8, u8) {
    assert!(x < axis && y < axis);
    let page = pages
        .iter()
        .find(|page| page.x == x / PAGE && page.y == y / PAGE)
        .expect("model page");
    let index = usize::from(y % PAGE) * usize::from(page.width) + usize::from(x % PAGE);
    let model = page.water_model.as_ref().expect("model output");
    (
        model.kind[index],
        model.surface_level_centimeters[index],
        model.flow_direction[index],
        model.provenance[index],
    )
}

fn polygon_for_cell(request: MapRequest, axis: u16, x: u16, y: u16) -> Vec<WaterCorrectionVertex> {
    let side = request.estimate().expect("estimate").effective_side_meters as f64;
    let spacing = side / f64::from(axis);
    let center_x = -side / 2.0 + (f64::from(x) + 0.5) * spacing;
    let center_y = side / 2.0 - (f64::from(y) + 0.5) * spacing;
    let mut local = SpatialRef::from_definition(&crate::local_aeqd_definition(
        request.center_latitude_e7,
        request.center_longitude_e7,
    ))
    .expect("local reference");
    let mut geographic = SpatialRef::from_epsg(4326).expect("geographic reference");
    local.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    geographic.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    let transform = CoordTransform::new(&local, &geographic).expect("inverse transform");
    let mut longitude = vec![
        center_x - 1_000.0,
        center_x + 1_000.0,
        center_x + 1_000.0,
        center_x - 1_000.0,
    ];
    let mut latitude = vec![
        center_y + 1_000.0,
        center_y + 1_000.0,
        center_y - 1_000.0,
        center_y - 1_000.0,
    ];
    transform
        .transform_coords(&mut longitude, &mut latitude, &mut [])
        .expect("patch projection");
    longitude
        .into_iter()
        .zip(latitude)
        .map(|(longitude, latitude)| WaterCorrectionVertex {
            longitude_e7: (longitude * 10_000_000.0).round() as i32,
            latitude_e7: (latitude * 10_000_000.0).round() as i32,
        })
        .collect()
}

#[test]
fn lakes_across_pages_share_one_level_and_river_junction_is_continuous() {
    let axis = 65;
    let mut kinds = vec![HydrologyKind::Land; usize::from(axis).pow(2)];
    let index = |x: u16, y: u16| usize::from(y) * usize::from(axis) + usize::from(x);
    kinds[index(63, 32)] = HydrologyKind::Lake;
    kinds[index(64, 32)] = HydrologyKind::Lake;
    kinds[index(62, 32)] = HydrologyKind::River;
    kinds[index(63, 31)] = HydrologyKind::Reservoir;
    kinds[index(0, 0)] = HydrologyKind::Ocean;
    let mut pages = pages_for_axis(axis, &kinds);
    let correction =
        WaterCorrectionDocument::empty(MapRequest::default(), axis).expect("empty corrections");
    let model = prepare_water_model(
        MapRequest::default(),
        &mut pages,
        &elevation_pages(axis, 1_234),
        correction,
    )
    .expect("water model");
    model.validate().expect("model index");
    for (x, y) in [(63, 32), (64, 32)] {
        let (kind, level, direction, provenance) = modeled_cell(&pages, axis, x, y);
        assert_eq!(kind, HydrologyKind::Lake as u8);
        assert_eq!(level, Some(1_234));
        assert_eq!(direction, WaterFlowDirection::Unknown as u8);
        assert_eq!(provenance, WaterModelProvenance::ModelledLakeSurface as u8);
    }
    let (river, river_level, river_flow, _) = modeled_cell(&pages, axis, 62, 32);
    assert_eq!(river, HydrologyKind::River as u8);
    assert_eq!(river_level, Some(1_234));
    assert_eq!(river_flow, WaterFlowDirection::Unknown as u8);
    let (ocean, ocean_level, _, ocean_provenance) = modeled_cell(&pages, axis, 0, 0);
    assert_eq!(ocean, HydrologyKind::Ocean as u8);
    assert_eq!(ocean_level, Some(0));
    assert_eq!(
        ocean_provenance,
        WaterModelProvenance::ModelledOceanSurface as u8
    );
    let (reservoir, level, _, provenance) = modeled_cell(&pages, axis, 63, 31);
    assert_eq!(reservoir, HydrologyKind::Reservoir as u8);
    assert_eq!(level, None);
    assert_eq!(provenance, WaterModelProvenance::EvidenceOnly as u8);
    for page in &pages {
        page.validate().expect("modeled evidence page");
    }
}

#[test]
fn correction_precedence_is_deterministic_and_corrected_land_is_distinct() {
    let request = MapRequest::default();
    let axis = 4;
    let polygon = polygon_for_cell(request, axis, 1, 1);
    let patches = vec![
        aoe_map::GeographicWaterPatch {
            id: "lake-first".to_owned(),
            precedence: 5,
            applies_from_year_ce: 500,
            applies_through_year_ce: 700,
            source_citation: "survey A".to_owned(),
            operation: WaterCorrectionOperation::SetNaturalLake,
            polygon: polygon.clone(),
        },
        aoe_map::GeographicWaterPatch {
            id: "land-last".to_owned(),
            precedence: 5,
            applies_from_year_ce: 500,
            applies_through_year_ce: 700,
            source_citation: "survey B".to_owned(),
            operation: WaterCorrectionOperation::SetLand,
            polygon,
        },
    ];
    let corrections =
        WaterCorrectionDocument::new(request, axis, patches).expect("validated corrections");
    assert_eq!(corrections.patches[0].id, "lake-first");
    assert_eq!(corrections.patches[1].id, "land-last");
    let mut kinds = vec![HydrologyKind::Land; usize::from(axis).pow(2)];
    kinds[usize::from(axis) + 1] = HydrologyKind::Lake;
    let mut pages = pages_for_axis(axis, &kinds);
    let index = prepare_water_model(
        request,
        &mut pages,
        &elevation_pages(axis, 900),
        corrections,
    )
    .expect("water model");
    let (kind, level, _, provenance) = modeled_cell(&pages, axis, 1, 1);
    assert_eq!(kind, HydrologyKind::Land as u8);
    assert_eq!(level, None);
    assert_eq!(provenance, WaterModelProvenance::GeographicCorrection as u8);
    assert_eq!(index.correction_document.patches.len(), 2);
    pages[0].validate().expect("modeled page");
}

#[test]
fn explicit_set_land_is_recorded_even_when_the_modern_kind_is_already_land() {
    let request = MapRequest::default();
    let axis = 4;
    let corrections = WaterCorrectionDocument::new(
        request,
        axis,
        vec![aoe_map::GeographicWaterPatch {
            id: "historical-dry-land".to_owned(),
            precedence: 1,
            applies_from_year_ce: 500,
            applies_through_year_ce: 700,
            source_citation: "curated historical evidence".to_owned(),
            operation: WaterCorrectionOperation::SetLand,
            polygon: polygon_for_cell(request, axis, 1, 1),
        }],
    )
    .expect("validated correction");
    let kinds = vec![HydrologyKind::Land; usize::from(axis).pow(2)];
    let mut pages = pages_for_axis(axis, &kinds);
    prepare_water_model(
        request,
        &mut pages,
        &elevation_pages(axis, 100),
        corrections,
    )
    .expect("water model");

    let corrected = modeled_cell(&pages, axis, 1, 1);
    assert_eq!(corrected.0, HydrologyKind::Land as u8);
    assert_eq!(corrected.1, None);
    assert_eq!(
        corrected.3,
        WaterModelProvenance::GeographicCorrection as u8
    );
    pages[0].validate().expect("same-kind correction page");
}

#[test]
fn elevation_grid_rejects_out_of_bound_coordinates_before_allocation() {
    let mut page = elevation_pages(2, 100).remove(0);
    page.x = u16::MAX;
    page.y = u16::MAX;
    assert!(ElevationGrid::new(&[page]).is_err());
}

#[test]
fn processing_order_does_not_change_model_pages_or_roots() {
    let axis = 65;
    let mut kinds = vec![HydrologyKind::Land; usize::from(axis).pow(2)];
    kinds[32 * usize::from(axis) + 63] = HydrologyKind::Lake;
    kinds[32 * usize::from(axis) + 64] = HydrologyKind::Lake;
    let mut first = pages_for_axis(axis, &kinds);
    let mut second = first.clone();
    second.reverse();
    let elevation = elevation_pages(axis, 777);
    let corrections =
        WaterCorrectionDocument::empty(MapRequest::default(), axis).expect("empty corrections");
    prepare_water_model(
        MapRequest::default(),
        &mut first,
        &elevation,
        corrections.clone(),
    )
    .expect("first model");
    prepare_water_model(MapRequest::default(), &mut second, &elevation, corrections)
        .expect("second model");
    assert_eq!(
        aoe_map::ordered_hydrology_page_root(&first).expect("first root"),
        aoe_map::ordered_hydrology_page_root(&second).expect("second root")
    );
}

#[test]
fn correction_document_rejects_grid_and_request_mismatch() {
    let request = MapRequest::default();
    let corrections =
        WaterCorrectionDocument::new(request, 4, Vec::new()).expect("correction document");
    assert!(corrections.validate_for(request, 4).is_ok());
    assert!(corrections.validate_for(request, 5).is_err());
    assert!(
        corrections
            .validate_for(
                MapRequest {
                    seed: request.seed + 1,
                    ..request
                },
                4
            )
            .is_err()
    );
    assert_eq!(
        corrections.projection,
        WaterCorrectionProjection::LocalAeqdWgs84V1
    );
    let encoded = corrections.serialize().expect("serialized document");
    let decoded = WaterCorrectionDocument::deserialize(&encoded).expect("decoded document");
    assert_eq!(decoded, corrections);
    assert_eq!(
        decoded.digest().expect("digest"),
        corrections.digest().expect("digest")
    );
}

#[test]
fn directed_river_profiles_are_nonincreasing_and_leave_unsupported_flow_unknown() {
    let axis = 65;
    let mut kinds = vec![HydrologyKind::Land; usize::from(axis).pow(2)];
    let index = |x: u16, y: u16| usize::from(y) * usize::from(axis) + usize::from(x);
    for x in [62, 63, 64] {
        kinds[index(x, 32)] = HydrologyKind::River;
    }
    kinds[index(10, 10)] = HydrologyKind::River;
    let mut pages = pages_for_axis(axis, &kinds);
    let mut elevation = elevation_pages(axis, 777);
    elevation[0].geographic_height_centimeters[usize::from(32_u16) * usize::from(PAGE) + 62] = 100;
    elevation[0].geographic_height_centimeters[usize::from(32_u16) * usize::from(PAGE) + 63] = 250;
    elevation[1].geographic_height_centimeters[32] = 200;
    let mut topology = vec![None; usize::from(axis).pow(2)];
    topology[index(62, 32)] = Some(crate::hydrology::RiverTopologyCell {
        reach_id: 10,
        next_down_id: 20,
        distance_to_sink_centimeters: 3_000,
    });
    topology[index(63, 32)] = Some(crate::hydrology::RiverTopologyCell {
        reach_id: 10,
        next_down_id: 20,
        distance_to_sink_centimeters: 2_000,
    });
    topology[index(64, 32)] = Some(crate::hydrology::RiverTopologyCell {
        reach_id: 20,
        next_down_id: 0,
        distance_to_sink_centimeters: 1_000,
    });
    let corrections =
        WaterCorrectionDocument::empty(MapRequest::default(), axis).expect("empty corrections");
    prepare_water_model_with_river_topology(
        MapRequest::default(),
        &mut pages,
        &elevation,
        corrections,
        Some(&crate::hydrology::RiverTopologyGrid { cells: topology }),
    )
    .expect("directed water model");

    let first = modeled_cell(&pages, axis, 62, 32);
    let second = modeled_cell(&pages, axis, 63, 32);
    let third = modeled_cell(&pages, axis, 64, 32);
    assert_eq!(first.1, Some(250));
    assert_eq!(second.1, Some(250));
    assert_eq!(third.1, Some(200));
    assert!(first.1.expect("first level") >= second.1.expect("second level"));
    assert!(second.1.expect("second level") >= third.1.expect("third level"));
    assert_eq!(
        first.2,
        WaterFlowDirection::East as u8,
        "upstream reach cell points to its downstream neighbor"
    );
    assert_eq!(
        second.2,
        WaterFlowDirection::East as u8,
        "reach junction follows NEXT_DOWN"
    );
    assert_eq!(third.2, WaterFlowDirection::Unknown as u8);
    assert_eq!(modeled_cell(&pages, axis, 10, 10).1, Some(777));
    assert_eq!(
        modeled_cell(&pages, axis, 10, 10).2,
        WaterFlowDirection::Unknown as u8
    );
}

#[test]
fn conflicting_river_profile_keeps_fixed_lake_junction_and_drops_direction() {
    let axis = 4;
    let index = |x: u16, y: u16| usize::from(y) * usize::from(axis) + usize::from(x);
    let mut kinds = vec![HydrologyKind::Land; usize::from(axis).pow(2)];
    kinds[index(1, 1)] = HydrologyKind::Lake;
    kinds[index(2, 1)] = HydrologyKind::River;
    kinds[index(3, 1)] = HydrologyKind::River;
    let mut pages = pages_for_axis(axis, &kinds);
    let mut elevation = elevation_pages(axis, 10);
    elevation[0].geographic_height_centimeters[index(3, 1)] = 20;
    let mut topology = vec![None; kinds.len()];
    topology[index(2, 1)] = Some(crate::hydrology::RiverTopologyCell {
        reach_id: 10,
        next_down_id: 20,
        distance_to_sink_centimeters: 300,
    });
    topology[index(3, 1)] = Some(crate::hydrology::RiverTopologyCell {
        reach_id: 20,
        next_down_id: 0,
        distance_to_sink_centimeters: 200,
    });
    let corrections =
        WaterCorrectionDocument::empty(MapRequest::default(), axis).expect("empty corrections");
    prepare_water_model_with_river_topology(
        MapRequest::default(),
        &mut pages,
        &elevation,
        corrections,
        Some(&crate::hydrology::RiverTopologyGrid { cells: topology }),
    )
    .expect("water model with contradictory profile");

    let junction = modeled_cell(&pages, axis, 2, 1);
    assert_eq!(junction.1, Some(10));
    assert_eq!(junction.2, WaterFlowDirection::Unknown as u8);
    assert_eq!(
        junction.3,
        WaterModelProvenance::ModelledJunctionSurface as u8
    );
    assert_eq!(modeled_cell(&pages, axis, 3, 1).1, Some(20));
}
