//! Count actual production work, independently per test thread.
use super::super::*;
use std::cell::Cell;

thread_local! {
    static WORK: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}
pub(crate) fn record(kind: Option<bool>) -> (usize, usize) {
    WORK.with(|c| {
        let (n, l) = c.get();
        c.set(match kind {
            Some(false) => (n + 1, l),
            Some(true) => (n, l + 1),
            None => (0, 0),
        });
        c.get()
    })
}

#[test]
fn chunk_sized_fixture_bounds_noise_and_lattice_work_for_every_material() {
    use GroundMaterial::*;
    // At most four wear fields plus two lazy lush fields, four corners each.
    for (material, cap) in [
        (TemperateGrass, 6),
        (DryGrass, 6),
        (LushGrass, 4),
        (Sand, 4),
        (Rock, 4),
        (ForestFloor, 0),
        (Dirt, 0),
        (Mud, 0),
        (Snow, 0),
        (Ice, 0),
        (Shore, 0),
        (Water, 0),
        (DeepWater, 0),
        (Ford, 0),
        (GrassDirt, 0),
        (WornGrass, 0),
        (Gravel, 0),
    ] {
        for band in [
            NativeHeightBand::Lowland,
            NativeHeightBand::Montane,
            NativeHeightBand::Subalpine,
            NativeHeightBand::Alpine,
            NativeHeightBand::Nival,
        ] {
            for use_ in [LandUse::Uncleared, LandUse::Grazing, LandUse::Crop] {
                let mut total = (0, 0);
                for y in 32..32 + crate::CHUNK_TILES {
                    for x in -32..-32 + crate::CHUNK_TILES {
                        record(None);
                        vary(17, TileCoord::new(x, y), material, band, use_);
                        let (noise, lattice) = WORK.with(Cell::get);
                        assert!(noise <= cap, "{material:?} {band:?}: {noise} > {cap}");
                        assert_eq!(lattice, 4 * noise);
                        if cap == 0 {
                            assert_eq!((noise, lattice), (0, 0));
                        } else {
                            assert!(noise >= 4);
                        }
                        total.0 += noise;
                        total.1 += lattice;
                    }
                }
                assert!(total.0 <= cap * 1024 && total.1 <= cap * 4096);
                eprintln!("variety work {material:?} {band:?} {use_:?}: {total:?}");
            }
        }
    }
}
