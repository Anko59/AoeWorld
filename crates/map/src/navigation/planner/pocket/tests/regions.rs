use super::*;
use crate::MapChunkGenerator;

fn advanced_planner() -> RoutePlanner {
    let mut planner = RoutePlanner::new(TileCoord::new(10, 10), TileCoord::new(220, 10), 65_536);
    planner.leg_origin = TileCoord::new(106, 10);
    planner.work = 1_000;
    planner
}

#[test]
fn a_region_witness_skips_connectivity_but_keeps_difficult_route_guidance() {
    let generator = MapChunkGenerator::new([7; 32], 0, 256);
    let overlay = ResourceOverlay::default();
    let mut planner = advanced_planner();
    assert!(planner.needs_guidance(&generator, &overlay));
    let (from, to) = planner.pending_region_proof(&overlay).unwrap();
    planner.record_region_proof(from, to, RegionReachability::Straight, &overlay);
    assert!(!planner.needs_guidance(&generator, &overlay));
    assert_eq!(planner.pending_region_proof(&overlay), None);
    planner.leg_work = 16_384;
    planner.work = 17_384;
    assert!(planner.needs_guidance(&generator, &overlay));
}

#[test]
fn negative_or_unknown_regions_never_cancel_or_disable_path_search() {
    let generator = MapChunkGenerator::new([7; 32], 0, 256);
    for answer in [RegionReachability::Unreachable, RegionReachability::Unknown] {
        let mut planner = advanced_planner();
        let overlay = ResourceOverlay::default();
        let (from, to) = planner.pending_region_proof(&overlay).unwrap();
        planner.record_region_proof(from, to, answer, &overlay);
        assert_eq!(planner.pending_region_proof(&overlay), None);
        assert!(planner.needs_guidance(&generator, &overlay));
        assert!(!planner.pocket_checked);
        assert_eq!(planner.terminal, None);
    }
}

#[test]
fn stale_collision_or_wrong_endpoint_cannot_discharge_current_proofs() {
    let generator = MapChunkGenerator::new([7; 32], 0, 256);
    let mut planner = advanced_planner();
    let mut overlay = ResourceOverlay::default();
    let (from, to) = planner.pending_region_proof(&overlay).unwrap();
    for (wrong_from, wrong_to) in [(planner.origin, to), (from, TileCoord::new(219, 10))] {
        planner.record_region_proof(wrong_from, wrong_to, RegionReachability::Straight, &overlay);
        assert!(planner.needs_guidance(&generator, &overlay));
        assert!(!planner.pocket_checked);
    }
    planner.record_region_proof(from, to, RegionReachability::Straight, &overlay);
    overlay
        .add_footprint(aoe_core::TileRect::from_xywh(120, 100, 1, 1))
        .unwrap();
    assert_eq!(planner.pending_region_proof(&overlay), Some((from, to)));
    assert!(!planner.pocket_checked);
    assert!(planner.needs_guidance(&generator, &overlay));
}

#[test]
fn opening_ground_keeps_a_positive_region_witness_valid() {
    let generator = MapChunkGenerator::new([7; 32], 0, 256);
    let mut planner = advanced_planner();
    let mut overlay = ResourceOverlay::default();
    let rect = aoe_core::TileRect::from_xywh(120, 100, 1, 1);
    overlay.add_footprint(rect).unwrap();
    let (from, to) = planner.pending_region_proof(&overlay).unwrap();
    planner.record_region_proof(from, to, RegionReachability::Straight, &overlay);
    overlay.remove_footprint(rect);
    assert_eq!(planner.pending_region_proof(&overlay), None);
    assert!(planner.pocket_checked);
    assert!(!planner.needs_guidance(&generator, &overlay));
}

#[test]
fn negative_or_unknown_witnesses_do_not_retry_the_optional_lookup_within_a_leg() {
    let generator = MapChunkGenerator::new([7; 32], 0, 256);
    for answer in [RegionReachability::Unreachable, RegionReachability::Unknown] {
        let mut planner = advanced_planner();
        let mut overlay = ResourceOverlay::default();
        let (from, to) = planner.pending_region_proof(&overlay).unwrap();
        planner.record_region_proof(from, to, answer, &overlay);
        overlay
            .add_footprint(aoe_core::TileRect::from_xywh(120, 100, 1, 1))
            .unwrap();
        assert_eq!(planner.pending_region_proof(&overlay), None);
        assert!(planner.needs_guidance(&generator, &overlay));
        assert_eq!(planner.terminal, None);
    }
}

#[test]
fn connected_regions_discharge_pocket_work_but_keep_corridor_guidance() {
    let generator = MapChunkGenerator::new([7; 32], 0, 256);
    let overlay = ResourceOverlay::default();
    let mut planner = advanced_planner();
    let (from, to) = planner.pending_region_proof(&overlay).unwrap();
    planner.record_region_proof(from, to, RegionReachability::Connected, &overlay);
    assert!(planner.pocket_checked);
    assert!(planner.needs_guidance(&generator, &overlay));
    assert_eq!(planner.terminal, None);
}

#[test]
fn continuation_reuses_the_pocket_proof_but_needs_its_own_straight_witness() {
    let generator = MapChunkGenerator::new([7; 32], 0, 256);
    let mut overlay = ResourceOverlay::default();
    let mut planner = advanced_planner();
    let (from, to) = planner.pending_region_proof(&overlay).unwrap();
    planner.record_region_proof(from, to, RegionReachability::Straight, &overlay);
    planner.leg_origin = TileCoord::new(154, 10);
    assert_eq!(
        planner.pending_region_proof(&overlay),
        Some((planner.leg_origin, to))
    );
    assert!(planner.pocket_checked);
    assert!(planner.needs_guidance(&generator, &overlay));
    overlay
        .add_footprint(aoe_core::TileRect::from_xywh(120, 100, 1, 1))
        .unwrap();
    planner.pending_region_proof(&overlay);
    assert!(!planner.pocket_checked);
}
