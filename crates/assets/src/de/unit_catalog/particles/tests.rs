use super::*;

#[test]
fn real_naval_and_siege_particles_decode_painted_and_transparent_cells() {
    let Some(root) = std::env::var_os("AOE_DE_DIR") else {
        return;
    };
    let root = Path::new(&root);
    let catalog = super::super::resolve(root).unwrap();
    for pose in [
        Pose::FireStream,
        Pose::DemolitionBlast,
        Pose::PetardBlast,
        Pose::CannonGalleonMuzzle,
        Pose::BombardCannonMuzzle,
        Pose::HandCannoneerMuzzle,
        Pose::TrebuchetImpact,
        Pose::ProjectileImpact,
    ] {
        let chosen = catalog.iter().find(|s| s.pose == pose as usize).unwrap();
        let sheet = chosen.particle.as_ref().unwrap();
        let textures = sheet.load(root).unwrap();
        let mut painted = false;
        let mut transparent = false;
        for i in 0..usize::from(chosen.frames) {
            let frame = sheet.frame(&textures, i).unwrap();
            let (pixels, _) = frame.color.as_chunks::<4>();
            painted |= pixels.iter().any(|p| p[3] > 0);
            transparent |= pixels.iter().any(|p| p[3] == 0);
        }
        assert!(painted && transparent, "{pose:?}");
    }
}
