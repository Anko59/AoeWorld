use super::*;
use gdal::{
    Dataset, DriverManager,
    raster::Buffer,
    spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef},
    vector::{Feature, Geometry, LayerAccess, LayerOptions, OGRFieldType, OGRwkbGeometryType},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn offline_vector_pages(
    request: MapRequest,
    axis: u16,
    ocean: Vec<u8>,
) -> (
    Vec<HydrologyPage>,
    Vec<ModernLandCoverPage>,
    Option<RiverTopologyGrid>,
) {
    let directory = TestDirectory::new();
    let mut sampler = Sampler {
        request,
        axis,
        tiles: Vec::new(),
        ocean,
        lakes: lake_dataset(&directory),
        rivers: Some(river_dataset_with_terminal_connection(&directory, true)),
        river_reaches: Default::default(),
        river_cells: vec![None; usize::from(axis).pow(2)],
        river_topology: None,
    };
    let (water, modern) = sampler.pages().expect("offline vector sampling");
    assert!(!directory.path("worldcover.tif").exists());
    (water, modern, sampler.take_river_topology().unwrap())
}

const AXIS: u16 = 16;
const SAMPLE_SPACING_METERS: f64 = 1_875.0;

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "aoe-hydrology-sampler-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary sampler directory");
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn request() -> MapRequest {
    MapRequest::default()
}

fn worldcover_tile(directory: &TestDirectory, epsg: u32) -> OpenTile {
    let path = directory.path("worldcover.tif");
    let driver = DriverManager::get_driver_by_name("GTiff").expect("GTiff driver");
    let mut dataset = driver
        .create_with_band_type::<u8, _>(&path, 300, 300, 1)
        .expect("WorldCover raster");
    dataset
        .set_geo_transform(&[1.0, 0.01, 0.0, 51.0, 0.0, -0.01])
        .expect("WorldCover transform");
    dataset
        .set_spatial_ref(&SpatialRef::from_epsg(epsg).expect("WorldCover spatial reference"))
        .expect("WorldCover CRS");
    dataset
        .rasterband(1)
        .expect("WorldCover band")
        .write(
            (0, 0),
            (300, 300),
            &mut Buffer::new((300, 300), vec![40; 300 * 300]),
        )
        .expect("WorldCover samples");
    dataset.flush_cache().expect("flush WorldCover raster");
    drop(dataset);
    OpenTile {
        latitude: 48,
        longitude: 2,
        dataset: Dataset::open(path).expect("open WorldCover fixture"),
    }
}

fn local_to_geographic(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let request = request();
    let definition =
        crate::local_aeqd_definition(request.center_latitude_e7, request.center_longitude_e7);
    let mut local = SpatialRef::from_definition(&definition).expect("local CRS");
    let mut geographic = SpatialRef::from_epsg(4326).expect("WGS84 CRS");
    local.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    geographic.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    let transform = CoordTransform::new(&local, &geographic).expect("local-to-WGS84 transform");
    let mut longitude = points.iter().map(|point| point.0).collect::<Vec<_>>();
    let mut latitude = points.iter().map(|point| point.1).collect::<Vec<_>>();
    transform
        .transform_coords(&mut longitude, &mut latitude, &mut [])
        .expect("transform fixture geometry");
    longitude.into_iter().zip(latitude).collect()
}

fn polygon_wkt(bounds: (f64, f64, f64, f64)) -> String {
    let (left, bottom, right, top) = bounds;
    let geographic = local_to_geographic(&[
        (left, bottom),
        (right, bottom),
        (right, top),
        (left, top),
        (left, bottom),
    ]);
    let points = geographic
        .iter()
        .map(|(longitude, latitude)| format!("{longitude:.10} {latitude:.10}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("POLYGON (({points}))")
}

fn line_wkt(start: (f64, f64), end: (f64, f64)) -> String {
    let geographic = local_to_geographic(&[start, end]);
    let points = geographic
        .iter()
        .map(|(longitude, latitude)| format!("{longitude:.10} {latitude:.10}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("LINESTRING ({points})")
}

fn vector_dataset(path: &Path, name: &str, geometry_type: OGRwkbGeometryType::Type) -> Dataset {
    let driver = DriverManager::get_driver_by_name("GPKG").expect("GeoPackage driver");
    let mut dataset = driver.create_vector_only(path).expect("vector fixture");
    let mut spatial_ref = SpatialRef::from_epsg(4326).expect("fixture WGS84 CRS");
    spatial_ref.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    let _layer = dataset
        .create_layer(LayerOptions {
            name,
            srs: Some(&spatial_ref),
            ty: geometry_type,
            ..Default::default()
        })
        .expect("fixture layer");
    dataset
}

fn lake_dataset(directory: &TestDirectory) -> Dataset {
    let path = directory.path("lakes.gpkg");
    let dataset = vector_dataset(&path, "lakes", OGRwkbGeometryType::wkbPolygon);
    {
        let layer = dataset.layer(0).expect("lake layer");
        layer
            .create_defn_fields(&[("Lake_type", OGRFieldType::OFTInteger)])
            .expect("lake type field");
        for (bounds, kind) in [
            ((-6_000.0, 0.0, -3_000.0, 2_000.0), 1),
            ((-5_500.0, 0.0, -3_500.0, 2_000.0), 2),
            ((3_000.0, 0.0, 6_000.0, 2_000.0), 2),
            ((-2_000.0, 3_000.0, 2_000.0, 5_500.0), 3),
        ] {
            let mut feature = Feature::new(layer.defn()).expect("lake feature");
            feature.set_field_integer(0, kind).expect("set lake type");
            feature
                .set_geometry(Geometry::from_wkt(&polygon_wkt(bounds)).expect("lake geometry"))
                .expect("set lake geometry");
            feature.create(&layer).expect("write lake feature");
        }
    }
    drop(dataset);
    Dataset::open(path).expect("open lake fixture")
}

fn river_dataset(directory: &TestDirectory) -> Dataset {
    river_dataset_with_terminal_connection(directory, false)
}

fn river_dataset_with_terminal_connection(directory: &TestDirectory, connect: bool) -> Dataset {
    let path = directory.path("rivers.gpkg");
    let dataset = vector_dataset(&path, "rivers", OGRwkbGeometryType::wkbLineString);
    {
        let layer = dataset.layer(0).expect("river layer");
        layer
            .create_defn_fields(&[
                ("HYRIV_ID", OGRFieldType::OFTInteger64),
                ("NEXT_DOWN", OGRFieldType::OFTInteger64),
                ("DIST_DN_KM", OGRFieldType::OFTReal),
                ("DIS_AV_CMS", OGRFieldType::OFTReal),
            ])
            .expect("river topology fields");
        let mut reaches = vec![
            (10, 20, 10.0, (-8_000.0, 937.5), (0.0, 937.5)),
            (20, 0, 0.0, (0.0, 937.5), (8_000.0, 937.5)),
        ];
        if connect {
            // Reach 20 needs an actual downstream endpoint to establish orientation.
            // Keep the two-reach fixture unchanged otherwise.
            reaches[1].1 = 30;
            reaches[1].2 = 4.0;
            reaches.push((30, 0, 0.0, (8_000.0, 937.5), (12_000.0, 937.5)));
        }
        for (id, downstream_id, distance_to_sink_km, start, end) in reaches {
            let mut feature = Feature::new(layer.defn()).expect("river feature");
            feature.set_field_integer64(0, id).expect("set reach id");
            feature
                .set_field_integer64(1, downstream_id)
                .expect("set downstream reach id");
            feature
                .set_field_double(2, distance_to_sink_km)
                .expect("set downstream distance");
            feature.set_field_double(3, 2_500.0).expect("set discharge");
            feature
                .set_geometry(Geometry::from_wkt(&line_wkt(start, end)).expect("river line"))
                .expect("set river geometry");
            feature.create(&layer).expect("write river feature");
        }
    }
    drop(dataset);
    Dataset::open(path).expect("open river fixture")
}

fn sampler(directory: &TestDirectory, worldcover_epsg: u32) -> Sampler {
    let length = usize::from(AXIS).pow(2);
    let mut ocean = vec![0; length];
    ocean[0] = 50;
    Sampler {
        request: request(),
        axis: AXIS,
        tiles: vec![worldcover_tile(directory, worldcover_epsg)],
        ocean,
        lakes: lake_dataset(directory),
        rivers: Some(river_dataset(directory)),
        river_reaches: Default::default(),
        river_cells: vec![None; length],
        river_topology: None,
    }
}

#[test]
fn sampler_pages_classify_vector_evidence_and_resolve_connected_river_reaches() {
    assert_eq!(SAMPLE_SPACING_METERS, 1_875.0);
    let directory = TestDirectory::new();
    let mut sampler = sampler(&directory, 4326);
    let (water_pages, land_cover_pages) = sampler.pages().expect("sample vector-backed page");
    let water = &water_pages[0];
    let land_cover = &land_cover_pages[0];
    let index = |row: usize, column: usize| row * usize::from(AXIS) + column;

    assert_eq!(water.kind[index(0, 0)], HydrologyKind::Ocean as u8);
    assert_eq!(
        water.method[index(0, 0)],
        HydrologyEvidenceMethod::OverviewOcean as u8
    );
    assert_eq!(water.kind[index(7, 5)], HydrologyKind::Lake as u8);
    assert_eq!(water.kind[index(7, 10)], HydrologyKind::Reservoir as u8);
    assert_eq!(water.kind[index(5, 7)], HydrologyKind::RegulatedLake as u8);
    assert_eq!(water.kind[index(5, 8)], HydrologyKind::RegulatedLake as u8);
    assert_eq!(water.kind[index(7, 4)], HydrologyKind::River as u8);
    assert_eq!(
        water.method[index(7, 4)],
        HydrologyEvidenceMethod::HydroRiversBufferedCorridor as u8
    );
    assert_eq!(water.kind[index(7, 6)], HydrologyKind::River as u8);
    assert_eq!(water.kind[index(7, 8)], HydrologyKind::River as u8);
    assert_eq!(water.kind[index(7, 11)], HydrologyKind::River as u8);
    assert_eq!(water.kind[index(1, 1)], HydrologyKind::Land as u8);
    assert!(land_cover.worldcover_class.iter().all(|class| *class == 40));

    let topology = sampler.river_topology.as_ref().expect("connected reaches");
    let upstream = topology.cells[index(7, 4)].expect("upstream reach cell");
    let downstream = topology.cells[index(7, 6)].expect("second upstream reach cell");
    assert_eq!((upstream.reach_id, upstream.next_down_id), (10, 20));
    assert_eq!((downstream.reach_id, downstream.next_down_id), (10, 20));
    assert!(upstream.distance_to_sink_centimeters > downstream.distance_to_sink_centimeters);
}

#[test]
fn vector_only_pages_never_open_worldcover_and_keep_unobserved_cells() {
    let directory = TestDirectory::new();
    let length = usize::from(AXIS).pow(2);
    let mut sampler = Sampler {
        request: request(),
        axis: AXIS,
        tiles: Vec::new(),
        ocean: vec![0; length],
        lakes: lake_dataset(&directory),
        rivers: Some(river_dataset_with_terminal_connection(&directory, true)),
        river_reaches: Default::default(),
        river_cells: vec![None; length],
        river_topology: None,
    };
    let (mut water, modern) = sampler.pages().expect("vector-only sampling");
    assert!(!directory.path("worldcover.tif").exists());
    assert!(modern[0].worldcover_class.iter().all(|class| *class == 0));
    let index = |row: usize, column: usize| row * usize::from(AXIS) + column;
    assert_eq!(water[0].kind[index(1, 1)], HydrologyKind::NoEvidence as u8);
    assert_eq!(
        water[0].method[index(1, 1)],
        HydrologyEvidenceMethod::None as u8
    );
    assert_eq!(water[0].kind[index(7, 5)], HydrologyKind::Lake as u8);
    assert_eq!(water[0].kind[index(7, 10)], HydrologyKind::Reservoir as u8);
    assert_eq!(
        water[0].kind[index(5, 7)],
        HydrologyKind::RegulatedLake as u8
    );
    assert_eq!(water[0].kind[index(7, 4)], HydrologyKind::River as u8);
    let topology = sampler
        .river_topology
        .as_ref()
        .expect("connected vector topology");
    let reach20 = topology.cells[index(7, 8)].expect("oriented reach 20");
    assert_eq!((reach20.reach_id, reach20.next_down_id), (20, 30));
    assert!(
        topology.cells[index(7, 9)]
            .expect("downstream cell")
            .distance_to_sink_centimeters
            < reach20.distance_to_sink_centimeters
    );
    assert_eq!(water[0].kind[index(7, 12)], HydrologyKind::River as u8);
    assert!(
        topology.cells[index(7, 12)].is_none(),
        "terminal reach has no proven orientation"
    );
    let original_kind = water[0].kind.clone();
    let original_method = water[0].method.clone();
    let elevation = vec![aoe_map::ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: AXIS as u8,
        height: AXIS as u8,
        geographic_height_centimeters: vec![12_000; length],
    }];
    let model_index = super::super::water_model::prepare_water_model_with_river_topology(
        request(),
        &mut water,
        &elevation,
        aoe_map::WaterCorrectionDocument::empty(request(), AXIS).expect("corrections"),
        sampler.river_topology.as_ref(),
    )
    .expect("model vector-only water");
    assert_eq!(water[0].kind, original_kind);
    assert_eq!(water[0].method, original_method);
    let model = water[0].water_model.as_ref().expect("model");
    assert_eq!(model.kind[index(1, 1)], HydrologyKind::NoEvidence as u8);
    assert_eq!(model.surface_level_centimeters[index(1, 1)], None);
    assert_eq!(
        model.provenance[index(1, 1)],
        aoe_map::WaterModelProvenance::EvidenceOnly as u8
    );
    assert_eq!(
        model.flow_direction[index(7, 8)],
        aoe_map::WaterFlowDirection::East as u8
    );
    assert_eq!(
        model.provenance[index(7, 8)],
        aoe_map::WaterModelProvenance::ModelledRiverSurface as u8
    );
    assert_eq!(
        model.flow_direction[index(7, 12)],
        aoe_map::WaterFlowDirection::Unknown as u8
    );
    assert_eq!(model.kind[index(7, 10)], HydrologyKind::Reservoir as u8);
    assert_eq!(model.surface_level_centimeters[index(7, 10)], None);
    assert_eq!(
        model.flow_direction[index(7, 10)],
        aoe_map::WaterFlowDirection::Unknown as u8
    );
    assert_eq!(model.kind[index(5, 7)], HydrologyKind::RegulatedLake as u8);
    assert_eq!(model.surface_level_centimeters[index(5, 7)], None);
    assert_eq!(
        model.provenance[index(7, 5)],
        aoe_map::WaterModelProvenance::ModelledLakeSurface as u8
    );
    let evidence_index = aoe_map::HydrologyEvidenceIndex {
        samples_per_axis: AXIS,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        world_cover_year: aoe_map::WORLD_COVER_OBSERVATION_YEAR,
        policy: aoe_map::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: aoe_map::ordered_hydrology_page_root(&water).expect("vector root"),
        modern_land_cover_page_root: aoe_map::ordered_modern_land_cover_page_root(&modern)
            .expect("nodata root"),
        water_model: Some(model_index),
    };
    evidence_index
        .validate_pages(&water, &modern)
        .expect("nodata evidence pages");
    assert!(matches!(
        sampler.pages_cancelled(&std::sync::atomic::AtomicBool::new(true)),
        Err(GeodataError::Cache(crate::CacheError::Cancelled))
    ));
}

#[test]
fn sampler_pages_reject_a_worldcover_crs_mismatch_after_vector_sampling() {
    let directory = TestDirectory::new();
    let mut sampler = sampler(&directory, 3857);
    assert!(matches!(
        sampler.pages(),
        Err(GeodataError::Preparation(
            "WorldCover raster CRS is not EPSG:4326"
        ))
    ));
}

#[test]
fn sampler_constructor_rejects_missing_tiles_and_wrong_ocean_shape_before_io() {
    let request = request();
    let missing = Path::new("no-source-access-expected.zip");
    assert!(matches!(
        Sampler::new(
            request,
            AXIS,
            Vec::new(),
            vec![0; usize::from(AXIS).pow(2)],
            missing,
            None
        ),
        Err(GeodataError::Preparation(
            "no WorldCover tile intersects the request"
        ))
    ));
    assert!(matches!(
        Sampler::new(
            request,
            AXIS,
            vec![Tile {
                latitude: 48,
                longitude: 2,
                path: PathBuf::from("no-worldcover-read-expected.tif"),
            }],
            vec![0; usize::from(AXIS).pow(2) - 1],
            missing,
            None,
        ),
        Err(GeodataError::Preparation(
            "resampled ocean coverage does not match the hydrology grid"
        ))
    ));
}
