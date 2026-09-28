use super::{
    SourceScalePackageReference,
    metrics::ProcessRssSampler,
    navigation_report::SourceScaleEvidence,
    report::{SourceWorkloadContract, WorkloadDatasetEvidence, WorkloadEvidenceClass},
};
use crate::{PageResidency, load_map_packages};
use aoe_map::{CHUNK_TILES, GAME_TILE_METERS, LayerProvenance, MapPackage};

const SOURCE_WORKLOAD_AXES: [u64; 4] = [512, 16_384, 50_000, 262_144];
#[cfg(test)]
const MAX_TILES_PER_SIDE: u64 = 262_144;
const REQUIRED_SCALE_AXES: [u64; 4] = SOURCE_WORKLOAD_AXES;

#[derive(Debug, thiserror::Error)]
pub enum ScaleQualificationError {
    #[error(
        "source scale package axes {actual:?} are invalid; include the primary 50,000 case and use unique supported axes"
    )]
    ReferenceConfiguration { actual: Vec<u64> },
    #[error(
        "source scale package `{package_hash}` is not a recipe-5/6 source-backed {tiles_per_side}-tile 1:1 package"
    )]
    UnsupportedPackage {
        tiles_per_side: u64,
        package_hash: String,
    },
    #[error("source scale package `{requested}` is absent; verified packages: {available:?}")]
    UnknownPackage {
        requested: String,
        available: Vec<String>,
    },
    #[error(
        "source scale {tiles_per_side} sampled {sampled_tile_count} tiles and read {verified_page_load_count} verified pages"
    )]
    EmptySample {
        tiles_per_side: u64,
        sampled_tile_count: usize,
        verified_page_load_count: u64,
    },
    #[error(
        "source scale {tiles_per_side} residency reached {observed_pages} pages, above the fixed {maximum_pages}-page bound"
    )]
    ResidentPageLimit {
        tiles_per_side: u64,
        observed_pages: usize,
        maximum_pages: usize,
    },
    #[error(transparent)]
    Store(#[from] crate::MapStoreError),
    #[error(transparent)]
    Package(#[from] aoe_map::MapPackageError),
    #[error(transparent)]
    Page(#[from] aoe_map::EnvironmentPageError),
}

pub(super) fn qualify_scale_packages(
    references: &[SourceScalePackageReference],
    rss: &mut ProcessRssSampler,
) -> Result<Vec<SourceScaleEvidence>, ScaleQualificationError> {
    let mut sorted = references.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|reference| reference.tiles_per_side);
    let actual_axes = sorted
        .iter()
        .map(|reference| reference.tiles_per_side)
        .collect::<Vec<_>>();
    if actual_axes.is_empty()
        || !actual_axes.contains(&50_000)
        || actual_axes.windows(2).any(|pair| pair[0] == pair[1])
        || actual_axes
            .iter()
            .any(|axis| !REQUIRED_SCALE_AXES.contains(axis))
    {
        return Err(ScaleQualificationError::ReferenceConfiguration {
            actual: actual_axes,
        });
    }
    sorted
        .into_iter()
        .map(|reference| qualify_scale_package(reference, rss))
        .collect()
}

fn qualify_scale_package(
    reference: &SourceScalePackageReference,
    rss: &mut ProcessRssSampler,
) -> Result<SourceScaleEvidence, ScaleQualificationError> {
    let packages = load_map_packages(Some(&reference.directory))?;
    let package = packages
        .get(&reference.content_hash)
        .cloned()
        .ok_or_else(|| ScaleQualificationError::UnknownPackage {
            requested: reference.content_hash.clone(),
            available: packages.keys().cloned().collect(),
        })?;
    validate_scale_package(&package, reference.tiles_per_side)?;

    let provider = PageResidency::open(&reference.directory, &package, &|| false)?;
    let generator = package.generator_with_page_provider(provider.clone())?;
    let chunks = sparse_viewport_chunks(reference.tiles_per_side);
    let mut sampled_tile_count = 0usize;
    let mut peak_resident_pages = provider.resident_pages();
    for [x, y] in &chunks {
        let chunk = generator.chunk_with_cancel(*x, *y, &|| false)?;
        sampled_tile_count = sampled_tile_count.saturating_add(chunk.tiles.len());
        peak_resident_pages = peak_resident_pages.max(provider.resident_pages());
        if peak_resident_pages > super::MAX_RESIDENT_PAGES {
            return Err(ScaleQualificationError::ResidentPageLimit {
                tiles_per_side: reference.tiles_per_side,
                observed_pages: peak_resident_pages,
                maximum_pages: super::MAX_RESIDENT_PAGES,
            });
        }
        rss.observe();
    }
    let verified_page_load_count = provider.verified_page_loads();
    if sampled_tile_count == 0 || verified_page_load_count == 0 {
        return Err(ScaleQualificationError::EmptySample {
            tiles_per_side: reference.tiles_per_side,
            sampled_tile_count,
            verified_page_load_count,
        });
    }
    let minimum_x = chunks.iter().map(|chunk| chunk[0]).min().unwrap_or(0);
    let maximum_x = chunks.iter().map(|chunk| chunk[0]).max().unwrap_or(0);
    let minimum_y = chunks.iter().map(|chunk| chunk[1]).min().unwrap_or(0);
    let maximum_y = chunks.iter().map(|chunk| chunk[1]).max().unwrap_or(0);
    Ok(SourceScaleEvidence {
        tiles_per_side: package.estimate.tiles_per_side,
        physical_side_meters: package.estimate.game_side_meters,
        package_hash: package.content_hash_hex(),
        generation_recipe_version: package.generation_recipe_version,
        sample_axis: package.environment.samples_per_axis,
        source_lock_count: package.source_locks.len(),
        indexed_page_count: provider.indexed_pages(),
        viewport_chunk_sequence: chunks,
        sampled_tile_count,
        verified_page_load_count,
        peak_resident_pages,
        viewport_extent_tiles: [
            (maximum_x - minimum_x) * CHUNK_TILES,
            (maximum_y - minimum_y) * CHUNK_TILES,
        ],
    })
}

fn validate_scale_package(
    package: &MapPackage,
    expected_axis: u64,
) -> Result<(), ScaleQualificationError> {
    let expected_side_meters = expected_axis * u64::from(GAME_TILE_METERS);
    if package.validate().is_err()
        || !super::supported_recipe(package.generation_recipe_version)
        || package.estimate.tiles_per_side != expected_axis
        || package.estimate.game_side_meters != expected_side_meters
        || package.request.requested_side_meters != expected_side_meters
        || package.request.compression.numerator != 1
        || package.request.compression.denominator != 1
        || package.source_locks.is_empty()
        || package.environment.samples_per_axis == 0
        || package.provenance.elevation != LayerProvenance::SourceDerived
    {
        return Err(ScaleQualificationError::UnsupportedPackage {
            tiles_per_side: expected_axis,
            package_hash: package.content_hash_hex(),
        });
    }
    Ok(())
}

fn sparse_viewport_chunks(tiles_per_side: u64) -> Vec<[i32; 2]> {
    let chunks_per_side = tiles_per_side.div_ceil(CHUNK_TILES as u64) as i32;
    let edge = chunks_per_side - 1;
    let center = chunks_per_side / 2;
    vec![
        [center, center],
        [0, 0],
        [edge, 0],
        [edge, edge],
        [0, edge],
        [center, center],
    ]
}

pub(super) fn source_workload_contracts(
    evidence: &[SourceScaleEvidence],
) -> Vec<SourceWorkloadContract> {
    SOURCE_WORKLOAD_AXES
        .into_iter()
        .map(|tiles_per_side| {
            let source_evidence = evidence
                .iter()
                .find(|item| item.tiles_per_side == tiles_per_side)
                .cloned();
            SourceWorkloadContract {
                tiles_per_side,
                physical_side_meters: tiles_per_side * u64::from(GAME_TILE_METERS),
                evidence_class: match tiles_per_side {
                    262_144 => WorkloadEvidenceClass::SparseMaximumContract,
                    _ if source_evidence.is_some() => {
                        WorkloadEvidenceClass::SourceBackedQualification
                    }
                    _ => WorkloadEvidenceClass::BoundedTestContract,
                },
                dataset_evidence: if source_evidence.is_some() {
                    WorkloadDatasetEvidence::VerifiedSourcePackage
                } else {
                    WorkloadDatasetEvidence::UnavailableNotClaimed
                },
                source_evidence,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_map::GENERATION_RECIPE_VERSION;
    use aoe_map::{MapRequest, Ratio};

    fn request(tiles_per_side: u64) -> MapRequest {
        MapRequest {
            requested_side_meters: tiles_per_side * u64::from(GAME_TILE_METERS),
            compression: Ratio::new(1, 1).expect("identity compression"),
            ..MapRequest::default()
        }
    }

    fn evidence(axis: u64) -> SourceScaleEvidence {
        SourceScaleEvidence {
            tiles_per_side: axis,
            physical_side_meters: axis * u64::from(GAME_TILE_METERS),
            package_hash: format!("{:064x}", axis),
            generation_recipe_version: GENERATION_RECIPE_VERSION,
            sample_axis: 128,
            source_lock_count: 7,
            indexed_page_count: 20,
            viewport_chunk_sequence: vec![[8, 8], [0, 0], [15, 0], [15, 15], [0, 15], [8, 8]],
            sampled_tile_count: 6_144,
            verified_page_load_count: 18,
            peak_resident_pages: 12,
            viewport_extent_tiles: [480, 480],
        }
    }

    #[test]
    fn source_workload_contracts_bound_sparse_cases_without_source_claims() {
        let contracts = source_workload_contracts(&[]);
        assert_eq!(
            contracts
                .iter()
                .map(|contract| contract.tiles_per_side)
                .collect::<Vec<_>>(),
            SOURCE_WORKLOAD_AXES
        );
        assert_eq!(contracts.len(), 4);
        assert_eq!(
            contracts[2].evidence_class,
            WorkloadEvidenceClass::BoundedTestContract
        );
        assert_eq!(
            contracts[3].evidence_class,
            WorkloadEvidenceClass::SparseMaximumContract
        );
        assert!(
            contracts.iter().all(|contract| contract.dataset_evidence
                == WorkloadDatasetEvidence::UnavailableNotClaimed)
        );

        for contract in contracts {
            let package = MapPackage::new(1, request(contract.tiles_per_side), Vec::new())
                .expect("bounded sparse package contract");
            assert_eq!(package.estimate.tiles_per_side, contract.tiles_per_side);
            assert_eq!(
                package.estimate.game_side_meters,
                contract.physical_side_meters
            );
            assert_eq!(package.environment.samples_per_axis, 0);
            assert!(contract.tiles_per_side <= MAX_TILES_PER_SIDE);
        }
    }

    #[test]
    fn sparse_viewport_sequence_visits_center_boundaries_and_returns() {
        for axis in REQUIRED_SCALE_AXES {
            let sequence = sparse_viewport_chunks(axis);
            let edge = axis.div_ceil(CHUNK_TILES as u64) as i32 - 1;
            assert_eq!(sequence.len(), 6);
            assert_eq!(sequence.first(), sequence.last());
            assert_eq!(sequence[1], [0, 0]);
            assert_eq!(sequence[2], [edge, 0]);
            assert_eq!(sequence[3], [edge, edge]);
            assert_eq!(sequence[4], [0, edge]);
            assert!(
                sequence
                    .iter()
                    .all(|[x, y]| { (0..=edge).contains(x) && (0..=edge).contains(y) })
            );
        }
    }

    #[test]
    fn source_scale_evidence_is_attached_without_overstating_maximum_generation() {
        let evidence = vec![evidence(512), evidence(50_000), evidence(262_144)];
        let contracts = source_workload_contracts(&evidence);
        assert_eq!(
            contracts[0].evidence_class,
            WorkloadEvidenceClass::SourceBackedQualification
        );
        assert_eq!(
            contracts[1].dataset_evidence,
            WorkloadDatasetEvidence::UnavailableNotClaimed
        );
        assert_eq!(
            contracts[2].dataset_evidence,
            WorkloadDatasetEvidence::VerifiedSourcePackage
        );
        assert_eq!(
            contracts[3].evidence_class,
            WorkloadEvidenceClass::SparseMaximumContract
        );
        assert_eq!(
            contracts[3].dataset_evidence,
            WorkloadDatasetEvidence::VerifiedSourcePackage
        );
        assert_eq!(
            contracts[3]
                .source_evidence
                .as_ref()
                .unwrap()
                .tiles_per_side,
            262_144
        );
    }

    #[test]
    fn scale_reference_axes_reject_duplicates_and_unknown_sizes() {
        let mut rss = ProcessRssSampler::start();
        let duplicate = [
            SourceScalePackageReference {
                tiles_per_side: 50_000,
                directory: "unused".into(),
                content_hash: "a".repeat(64),
            },
            SourceScalePackageReference {
                tiles_per_side: 50_000,
                directory: "unused".into(),
                content_hash: "b".repeat(64),
            },
        ];
        assert!(matches!(
            qualify_scale_packages(&duplicate, &mut rss),
            Err(ScaleQualificationError::ReferenceConfiguration { .. })
        ));
        let unknown = [
            SourceScalePackageReference {
                tiles_per_side: 50_000,
                directory: "unused".into(),
                content_hash: "a".repeat(64),
            },
            SourceScalePackageReference {
                tiles_per_side: 1_024,
                directory: "unused".into(),
                content_hash: "b".repeat(64),
            },
        ];
        assert!(matches!(
            qualify_scale_packages(&unknown, &mut rss),
            Err(ScaleQualificationError::ReferenceConfiguration { .. })
        ));
    }
}
