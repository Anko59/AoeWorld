use aoe_assets::{de, pack};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[path = "de_import/fixture.rs"]
mod fixture;

#[test]
fn real_all_unit_timelines_match_de_dat_and_complete_native_sld_cells() {
    let Some(root) = std::env::var_os("AOE_DE_DIR") else {
        return;
    };
    let root = Path::new(&root);
    let dat = aoe_assets::dat::de_survey::inflate(
        &fs::read(root.join("resources/_common/dat/empires2_x2_p1.dat")).unwrap(),
    )
    .unwrap();
    let units = aoe_assets::dat::de_units::find(&dat, &[4, 24, 7, 6]).unwrap();
    use aoe_assets::catalog::units::{Pose, release_frame};
    for (id, pose) in [
        (4, Pose::ArcherAttack),
        (24, Pose::CrossbowmanAttack),
        (7, Pose::SkirmisherAttack),
        (6, Pose::EliteSkirmisherAttack),
    ] {
        assert_eq!(
            u8::try_from(units[&id].release).ok(),
            release_frame(pose),
            "DAT release frame for {id}"
        );
    }
    let selected = de::unit_catalog::resolve(root).unwrap();
    assert_eq!(selected.len(), 164);
    let mut seen = std::collections::BTreeSet::new();
    for g in selected {
        let source = aoe_assets::catalog::units::SPRITES[g.pose];
        assert_eq!(source.id, g.sprite);
        assert_eq!(source.rows(), usize::from(g.angles));
        assert_eq!(
            u16::from(source.stride),
            if g.terminal { 1 } else { g.frames }
        );
        assert_eq!(
            source.frame_millis(),
            if g.terminal {
                0.0
            } else {
                f64::from(g.seconds) * 1000.0
            }
        );
        for part in g.parts {
            let a = part.animation;
            if !seen.insert(a.file.clone()) {
                continue;
            }
            let frames = aoe_assets::sld::parse(
                &fs::read(
                    root.join("resources/_common/drs/graphics")
                        .join(format!("{}.sld", a.file)),
                )
                .unwrap(),
            )
            .unwrap();
            assert!(
                frames.len() >= usize::from(a.frames) * usize::from(a.angles),
                "{}",
                a.file
            );
            assert!(frames.iter().any(|f| f.main.is_some()), "{}", a.file);
            if (130..135).contains(&g.pose) {
                assert!(frames.iter().any(|f| f.player.is_some()));
                for row in 0..usize::from(a.angles) {
                    let start = row * usize::from(a.frames);
                    assert!(
                        frames[start..start + usize::from(a.frames)]
                            .windows(2)
                            .any(|w| w[0] != w[1]),
                        "{} row {row}",
                        a.file
                    );
                }
            }
        }
    }
    assert!(seen.len() > 140);
}

#[test]
fn real_siege_stone_has_thirty_native_rotation_cells() {
    let Some(root) = std::env::var_os("AOE_DE_DIR") else {
        return;
    };
    let path = Path::new(&root).join("resources/_common/drs/graphics/p_mangonel_x1.sld");
    let frames = aoe_assets::sld::parse(&fs::read(path).unwrap()).unwrap();
    assert_eq!(frames.len(), 30);
    assert!(frames.iter().all(|f| f.main.is_some()));
    assert!(frames.windows(2).any(|w| w[0].main != w[1].main));
}

struct CurrentDirectory(PathBuf);

impl Drop for CurrentDirectory {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

/// One 8x8 frame with a single 4x4 BC1 main block at (4,4) and a player block.
fn sld() -> Vec<u8> {
    let mut file = b"SLDX".to_vec();
    file.extend_from_slice(&[4, 0, 1, 0, 0, 0, 16, 0, 0xFF, 0, 0, 0]);
    for value in [8u16, 8, 2, 3] {
        file.extend_from_slice(&value.to_le_bytes());
    }
    file.extend_from_slice(&[0b1_0001, 0, 0, 0]);
    let mut main = Vec::new();
    for value in [4u16, 4, 8, 8] {
        main.extend_from_slice(&value.to_le_bytes());
    }
    main.extend_from_slice(&[0, 0, 1, 0, 0, 1]);
    main.extend_from_slice(&[0x00, 0xF8, 0x1F, 0x00, 0, 0, 0, 0]);
    file.extend_from_slice(&((main.len() + 4) as u32).to_le_bytes());
    file.extend(main);
    while !(file.len() - 16).is_multiple_of(4) {
        file.push(0);
    }
    let mut mask = vec![0, 0, 1, 0, 0, 1];
    mask.extend_from_slice(&[119, 0, 0, 0, 0, 0, 0, 0]);
    file.extend_from_slice(&((mask.len() + 4) as u32).to_le_bytes());
    file.extend(mask);
    while !(file.len() - 16).is_multiple_of(4) {
        file.push(0);
    }
    file
}

fn dds() -> Vec<u8> {
    let mut bytes = vec![0u8; 128];
    bytes[..4].copy_from_slice(b"DDS ");
    bytes[4..8].copy_from_slice(&124u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&8u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&8u32.to_le_bytes());
    bytes[84..88].copy_from_slice(b"DXT1");
    for _ in 0..4 {
        bytes.extend_from_slice(&[0x00, 0xF8, 0x1F, 0x00, 0, 0, 0, 0]);
    }
    bytes
}

#[test]
fn synthetic_install_imports_into_a_verified_pack_with_previews_and_table() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(temporary.path()).expect("enter tempdir");
    let _restore = CurrentDirectory(original);
    let graphics = Path::new("install/resources/_common/drs/graphics");
    let textures = Path::new("install/resources/_common/terrain/textures/2x");
    fs::create_dir_all(graphics).unwrap();
    fs::create_dir_all(textures).unwrap();
    fixture::actors(Path::new("install"));
    fs::write(graphics.join("b_dark_house_age1_x1.sld"), sld()).unwrap();
    fs::write(textures.join("g_fm1.dds"), dds()).unwrap();

    let report = de::import(Path::new("install"), Path::new("local-assets/packs")).expect("import");
    let manifest = pack::verify(&report.pack).expect("verify");
    let native: Vec<_> = manifest
        .frames
        .iter()
        .filter(|f| f.native.is_some())
        .collect();
    assert_eq!(
        native.len(),
        90_132,
        "every native unit/stone source cell is chunked"
    );
    assert_eq!(
        manifest.chunks.len(),
        1,
        "synthetic identical cells deduplicate"
    );
    assert_eq!(
        manifest.pages.len(),
        1,
        "native timelines consume no PNG area"
    );
    let house = manifest
        .frames
        .iter()
        .find(|f| f.source == "de/b_dark_house_age1_x1.sld")
        .expect("house frame");
    // Cropped to the 4x4 layer at (4,4); the anchor (2,3) moves by the crop.
    assert_eq!((house.width, house.height), (4, 4));
    assert_eq!((house.anchor_x, house.anchor_y), (-2, -1));
    let page = &manifest.pages[usize::from(house.page)];
    let mut decoder = png::Decoder::new(std::io::BufReader::new(
        fs::File::open(report.pack.join(&page.player)).unwrap(),
    ))
    .read_info()
    .unwrap();
    let mut pixels = vec![0; decoder.output_buffer_size()];
    decoder.next_frame(&mut pixels).unwrap();
    let at = (usize::from(house.y) * usize::from(page.width) + usize::from(house.x)) * 4;
    assert_eq!(
        &pixels[at..at + 4],
        &[1, 0, 0, 119],
        "low player mask survives atlas serialization"
    );
    let farm = manifest
        .frames
        .iter()
        .find(|f| f.source == "de/g_fm1.dds")
        .expect("farm");
    assert_eq!((farm.width, farm.height), (4, 4));
    assert_eq!(report.entries.len(), 155);
    assert_eq!(
        manifest
            .frames
            .iter()
            .filter(|f| f.source == "de/p_mangonel_x1.sld")
            .count(),
        30
    );
    assert_eq!(
        manifest
            .frames
            .iter()
            .filter(|f| f.source
                == aoe_assets::catalog::units::SPRITES
                    [aoe_assets::catalog::units::Pose::MilitiaAttack as usize]
                    .manifest_source())
            .count(),
        481
    );
    use aoe_assets::catalog::units::{Pose, SPRITES};
    for pose in [
        Pose::GeneralStand,
        Pose::GeneralWalk,
        Pose::GeneralAttack,
        Pose::GeneralDie,
        Pose::GeneralDecay,
    ] {
        let source = SPRITES[pose as usize];
        assert_eq!(
            manifest
                .frames
                .iter()
                .filter(|f| f.source == source.manifest_source())
                .count(),
            source.frames() as usize
        );
    }
    assert!(
        !report.absent.is_empty(),
        "other selections are reported absent"
    );
    assert!(Path::new("local-assets/de-preview/b_dark_house_age1.png").is_file());
    let table = fs::read_to_string("local-assets/de-table.md").unwrap();
    assert!(table.contains("de/b_dark_house_age1_x1.sld"));
    // A second import is idempotent.
    assert_eq!(
        de::import(Path::new("install"), Path::new("local-assets/packs"))
            .unwrap()
            .pack,
        report.pack
    );
}

#[test]
fn a_directory_that_is_not_an_install_is_rejected() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(temporary.path()).expect("enter tempdir");
    let _restore = CurrentDirectory(original);
    fs::create_dir("empty").unwrap();
    assert!(de::import(Path::new("empty"), Path::new("local-assets/packs")).is_err());
}

#[test]
fn missing_native_siege_stone_is_rejected_before_pack_creation() {
    let temporary = tempfile::tempdir().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(temporary.path()).unwrap();
    let _restore = CurrentDirectory(original);
    fixture::actors(Path::new("install"));
    fs::remove_file("install/resources/_common/drs/graphics/p_mangonel_x1.sld").unwrap();
    let error = de::import(Path::new("install"), Path::new("local-assets/packs"))
        .expect_err("native siege art is required");
    assert!(error.to_string().contains("p_mangonel_x1.sld"), "{error}");
    assert!(!Path::new("local-assets/packs").exists());
}

#[test]
fn partial_native_siege_rotation_is_rejected_before_pack_creation() {
    let temporary = tempfile::tempdir().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(temporary.path()).unwrap();
    let _restore = CurrentDirectory(original);
    fixture::actors(Path::new("install"));
    let cell = sld();
    let mut bytes = cell[..16].to_vec();
    bytes[6..8].copy_from_slice(&29_u16.to_le_bytes());
    for _ in 0..29 {
        bytes.extend_from_slice(&cell[16..]);
    }
    fs::write(
        "install/resources/_common/drs/graphics/p_mangonel_x1.sld",
        bytes,
    )
    .unwrap();
    assert!(de::import(Path::new("install"), Path::new("local-assets/packs")).is_err());
    assert!(!Path::new("local-assets/packs").exists());
}

#[test]
fn missing_native_hero_pose_is_rejected_before_pack_creation() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(temporary.path()).expect("enter tempdir");
    let _restore = CurrentDirectory(original);
    fixture::actors(Path::new("install"));
    let graphic = aoe_assets::catalog::units::SPRITES
        [aoe_assets::catalog::units::Pose::GeneralAttack as usize]
        .id
        - 200_000;
    let file = format!("synthetic_{graphic}_x1.sld");
    fs::remove_file(Path::new("install/resources/_common/drs/graphics").join(&file)).unwrap();
    let error = de::import(Path::new("install"), Path::new("local-assets/packs"))
        .expect_err("hero attack is required");
    assert!(
        error.to_string().contains(file.trim_end_matches(".sld")),
        "{error}"
    );
    assert!(!Path::new("local-assets/packs").exists());
}

#[test]
fn zero_frame_sprite_is_rejected_before_pack_or_table_creation() {
    let temporary = tempfile::tempdir().expect("tempdir");
    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(temporary.path()).expect("enter tempdir");
    let _restore = CurrentDirectory(original);
    let graphics = Path::new("install/resources/_common/drs/graphics");
    fs::create_dir_all(graphics).unwrap();
    fixture::actors(Path::new("install"));
    fs::write(
        graphics.join("b_dark_house_age1_x1.sld"),
        [
            0x53, 0x4c, 0x44, 0x58, 4, 0, 0, 0, 0, 0, 0x10, 0, 0xff, 0, 0, 0,
        ],
    )
    .unwrap();
    let error = de::import(Path::new("install"), Path::new("local-assets/packs"))
        .expect_err("zero-frame sprite must be rejected");
    let message = error.to_string();
    assert!(
        message.contains("b_dark_house_age1_x1.sld") && message.contains("zero frames"),
        "{message}"
    );
    assert!(!Path::new("local-assets/packs").exists());
    assert!(!Path::new("local-assets/de-table.md").exists());
}

/// Run with `make test-assets-de [DE_DIR=<AoE2DE root>]`. Skipped in ordinary unit tests.
#[test]
fn real_house_decodes_with_plausible_dimensions() {
    let Some(root) = std::env::var_os("AOE_DE_DIR") else {
        eprintln!("AOE_DE_DIR unset: skipping the real-file test");
        return;
    };
    let path = Path::new(&root).join("resources/_common/drs/graphics/b_dark_house_age1_x1.sld");
    let frames = aoe_assets::sld::parse(&fs::read(path).unwrap()).expect("decodes");
    assert!(!frames.is_empty());
    let main = frames[0].main.as_ref().expect("main layer");
    assert!((100..=300).contains(&main.width) && (60..=200).contains(&main.height));
}
