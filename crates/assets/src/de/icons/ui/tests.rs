use super::*;

fn png_fixture(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(pixels)
            .unwrap();
    }
    bytes
}

#[test]
fn native_ui_png_keeps_dimensions_and_alpha() {
    let pixels = [10, 20, 30, 0, 40, 50, 60, 127];
    let bytes = png_fixture(2, 1, &pixels);
    let planes = decode(&bytes, true).unwrap();
    assert_eq!((planes.width, planes.height), (2, 1));
    assert_eq!(planes.color, pixels);
    assert!(planes.player.iter().all(|&value| value == 0));
    assert!(planes.shadow.iter().all(|&value| value == 0));
    assert!(planes.outline.iter().all(|&value| value == 0));
    assert!(decode(&bytes, false).is_err());
}

#[test]
fn native_ui_png_rejects_oversized_and_truncated_images() {
    let oversized = png_fixture(2049, 1, &vec![0; 2049 * 4]);
    assert!(decode(&oversized, true).is_err());
    let bytes = png_fixture(2, 1, &[255; 8]);
    assert!(decode(&bytes[..bytes.len() / 2], true).is_err());
}

fn background_fixture(width: u32, height: u32, blocks: &[[u8; 8]]) -> Vec<u8> {
    let mut bytes = vec![0; 128];
    bytes[..4].copy_from_slice(b"DDS ");
    bytes[4..8].copy_from_slice(&124_u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&height.to_le_bytes());
    bytes[16..20].copy_from_slice(&width.to_le_bytes());
    bytes[28..32].copy_from_slice(&1_u32.to_le_bytes());
    bytes[84..88].copy_from_slice(b"DXT1");
    for block in blocks {
        bytes.extend(block);
    }
    bytes
}

#[test]
fn native_menu_background_streams_one_average_pixel_per_bc1_block() {
    let red = [0, 0xf8, 0, 0, 0, 0, 0, 0];
    let green = [0xe0, 7, 0, 0, 0, 0, 0, 0];
    let bytes = background_fixture(8, 4, &[red, green]);
    let planes = background(&bytes).unwrap();
    assert_eq!((planes.width, planes.height), (2, 1));
    assert_eq!(planes.color, [255, 0, 0, 255, 0, 255, 0, 255]);
    // Two red and two blue rows must average all 16 pixels rather than sample one.
    let mixed = [0, 0xf8, 0x1f, 0, 0, 0, 0x55, 0x55];
    let planes = background(&background_fixture(4, 4, &[mixed])).unwrap();
    assert_eq!(planes.color, [127, 0, 127, 255]);
    let wide = background_fixture(5120, 4, &vec![[0; 8]; 1280]);
    let planes = background(&wide).unwrap();
    assert_eq!((planes.width, planes.height), (1280, 1));
    assert!(
        crate::dds::parse(&wide).is_err(),
        "the full DDS cap stays unchanged"
    );
}

#[test]
fn native_menu_background_bounds_source_layout_before_decoding() {
    for (width, height) in [(8196, 4), (4, 4100), (6, 4), (4, 6), (0, 4)] {
        assert!(background(&background_fixture(width, height, &[])).is_err());
    }
    let good = background_fixture(8, 4, &[[0; 8]; 2]);
    for end in [0, 3, 127, 128, good.len() - 1] {
        assert!(background(&good[..end]).is_err());
    }
    let mut unsupported = good.clone();
    unsupported[84..88].copy_from_slice(b"DXT5");
    assert!(background(&unsupported).is_err());
    let mut volume = good.clone();
    volume[24..28].copy_from_slice(&2_u32.to_le_bytes());
    assert!(background(&volume).is_err());
    let mut cube = good;
    cube[112..116].copy_from_slice(&0x200_u32.to_le_bytes());
    assert!(background(&cube).is_err());
}

fn source(id: &str, roles: Vec<Role>) -> Source {
    Source {
        id: id.into(),
        path: std::path::PathBuf::from("private/synthetic.png"),
        hash: "a".repeat(64),
        roles,
    }
}

fn manifest() -> Manifest {
    let hash = "a".repeat(64);
    serde_json::from_value(serde_json::json!({
        "version":1, "converter":"synthetic", "input_hash":hash,
        "pages":[{"color":"color-000.png", "color_hash":hash,
            "player":"player-000.png", "player_hash":hash,
            "shadow":"shadow-000.png", "shadow_hash":hash,
            "outline":"outline-000.png", "outline_hash":hash,
            "width":1024,"height":512}],
        "frames":[
            {"source":"de/interface/12.dds", "source_hash":hash,
                "frame":0,"page":0,"x":1,"y":2,"width":36,"height":36,
                "anchor_x":0,"anchor_y":0,"native":null},
            {"source":"de/interface/buildings/12.dds", "source_hash":hash,
                "frame":0,"page":0,"x":100,"y":2,"width":36,"height":36,
                "anchor_x":0,"anchor_y":0,"native":null},
            {"source":"de/interface/native-panel.png", "source_hash":hash,
                "frame":0,"page":0,"x":200,"y":2,"width":300,"height":172,
                "anchor_x":0,"anchor_y":0,"native":null}
        ],
        "chunks":[],"terrains":[],"buildings":[],"building_data":"00"
    }))
    .unwrap()
}

#[test]
fn native_ui_roles_export_building_icons_commands_resources_and_panels() {
    let sources = [
        source("de/interface/12.dds", vec![Role::Unit(83)]),
        source("de/interface/buildings/12.dds", vec![Role::Unit(70)]),
        source(
            "de/interface/native-panel.png",
            vec![
                Role::Command("stop"),
                Role::Resource("food"),
                Role::Panel("western-commands"),
                Role::Panel("celtic-commands"),
                Role::Panel("portrait"),
                Role::Panel("button"),
                Role::Panel("menu-background"),
            ],
        ),
    ];
    let mut out = Interface::default();
    apply(&manifest(), &sources, &mut out).unwrap();
    assert_eq!(out.units[&83].x, 1);
    assert_eq!(
        out.units[&70].x, 100,
        "equal sheet indices must stay separate"
    );
    for art in [
        &out.commands["stop"],
        &out.resources["food"],
        &out.panels["western-commands"],
        &out.panels["celtic-commands"],
        &out.panels["portrait"],
        &out.panels["button"],
        &out.panels["menu-background"],
    ] {
        assert_eq!(art.page, "color-000.png");
        assert_eq!((art.x, art.y, art.width, art.height), (200, 2, 300, 172));
        assert_eq!((art.page_width, art.page_height), (1024, 512));
    }
}

#[test]
fn native_ui_roles_require_the_exact_normal_frame_and_valid_page_crop() {
    let sources = [source("de/interface/12.dds", vec![Role::Unit(83)])];
    let check = |manifest: &Manifest| apply(manifest, &sources, &mut Interface::default());
    let mut missing = manifest();
    missing.frames[0].frame = 1;
    assert!(check(&missing).is_err());
    missing.frames[0].frame = 0;
    missing.frames[0].native = Some(crate::pack::frame_chunks::FrameLocation {
        chunk: 0,
        offset: 0,
        length: 4,
    });
    assert!(check(&missing).is_err());
    let mut page = manifest();
    page.frames[0].page = 10;
    assert!(check(&page).is_err());
    let mut crop = manifest();
    crop.frames[0].x = 1020;
    assert!(check(&crop).is_err());
    crop.frames[0].x = 1;
    crop.frames[0].width = 0;
    assert!(check(&crop).is_err());
}

#[test]
fn real_de_building_icons_and_interface_sources_decode() {
    let Some(root) = std::env::var_os("AOE_DE_DIR") else {
        return;
    };
    let mut hash = blake3::Hasher::new();
    let sources = resolve(std::path::Path::new(&root), &mut hash).unwrap();
    let mut groups = [false; 5];
    for source in &sources {
        let bytes = read_bounded(&source.path).unwrap();
        assert_eq!(blake3::hash(&bytes).to_hex().as_str(), source.hash);
        let planes = frame(source, &bytes)
            .unwrap_or_else(|error| panic!("{}: {error}", source.path.display()));
        assert!((1..=2048).contains(&planes.width), "{}", source.id);
        assert!((1..=2048).contains(&planes.height), "{}", source.id);
        assert_eq!(
            planes.color.len(),
            usize::from(planes.width) * usize::from(planes.height) * 4
        );
        for role in &source.roles {
            match role {
                Role::Unit(109) => groups[0] = true,
                Role::Command("stop") => groups[1] = true,
                Role::Resource("food") => groups[2] = true,
                Role::Panel("western-commands") => groups[3] = true,
                Role::Panel("menu-background") => groups[4] = true,
                _ => {}
            }
        }
    }
    assert_eq!(groups, [true; 5], "real DE UI inventory is incomplete");
}
