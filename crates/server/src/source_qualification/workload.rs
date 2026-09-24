use super::report::{SourceWorkloadContract, WorkloadDatasetEvidence, WorkloadEvidenceClass};
use aoe_map::GAME_TILE_METERS;
#[cfg(test)]
use aoe_map::{MapPackage, MapRequest, Ratio};

const SOURCE_WORKLOAD_AXES: [u64; 4] = [512, 16_384, 50_000, 262_144];
#[cfg(test)]
const MAX_TILES_PER_SIDE: u64 = 262_144;

pub(super) fn source_workload_contracts() -> Vec<SourceWorkloadContract> {
    SOURCE_WORKLOAD_AXES
        .into_iter()
        .map(|tiles_per_side| SourceWorkloadContract {
            tiles_per_side,
            physical_side_meters: tiles_per_side * u64::from(GAME_TILE_METERS),
            evidence_class: match tiles_per_side {
                50_000 => WorkloadEvidenceClass::SourceBackedQualification,
                262_144 => WorkloadEvidenceClass::SparseMaximumContract,
                _ => WorkloadEvidenceClass::BoundedTestContract,
            },
            dataset_evidence: if tiles_per_side == 50_000 {
                WorkloadDatasetEvidence::VerifiedSourcePackage
            } else {
                WorkloadDatasetEvidence::UnavailableNotClaimed
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(tiles_per_side: u64) -> MapRequest {
        MapRequest {
            requested_side_meters: tiles_per_side * u64::from(GAME_TILE_METERS),
            compression: Ratio::new(1, 1).expect("identity compression"),
            ..MapRequest::default()
        }
    }

    #[test]
    fn source_workload_contracts_bound_sparse_cases_without_source_claims() {
        let contracts = source_workload_contracts();
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
            WorkloadEvidenceClass::SourceBackedQualification
        );
        assert_eq!(
            contracts[3].evidence_class,
            WorkloadEvidenceClass::SparseMaximumContract
        );
        assert!(
            contracts
                .iter()
                .filter(|contract| contract.tiles_per_side != 50_000)
                .all(|contract| contract.dataset_evidence
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
}
