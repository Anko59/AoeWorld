use super::*;
#[test]
fn real_terrain_directory_and_every_native_resource_are_resolvable() {
    let Some(root) = std::env::var_os("AOE_DE_DIR") else {
        return;
    };
    let root = Path::new(&root);
    let catalog = de::terrain::resolve(root, &mut blake3::Hasher::new()).unwrap();
    assert_eq!(
        catalog.sheets.len(),
        85,
        "one native source per DAT filename"
    );
    assert!(
        catalog
            .terrains
            .iter()
            .any(|t| t.id == 40 && t.source.ends_with("g_rck.dds"))
    );
    assert!(
        catalog
            .terrains
            .iter()
            .any(|t| t.id == 124 && t.source.ends_with("g_sn2.dds"))
    );
    for sheet in &catalog.sheets {
        assert!(sheet.frames > 1);
        assert!(sheet.size.iter().all(|&n| n >= 512));
    }
    let c = de::buildings::resolve(root).unwrap();
    for id in aoe_assets::catalog::ENVIRONMENT_UNIT_IDS {
        let recipe = c.recipes.iter().find(|r| r.unit_id == id).unwrap();
        assert!(!recipe.parts.is_empty(), "native environment {id}");
        for p in recipe
            .parts
            .iter()
            .chain(recipe.states.iter().flat_map(|s| &s.parts))
        {
            assert!(p.available, "{}", p.source);
            let file = p.source.trim_start_matches("de/");
            let mut decoder = aoe_assets::sld::Stream::new(
                fs::read(root.join("resources/_common/drs/graphics").join(file)).unwrap(),
            )
            .unwrap();
            if id == 264 {
                assert_eq!(
                    (p.frames, p.angles, decoder.len()),
                    (1, 25, 24),
                    "DAT reserves one undrawn cliff variant"
                );
            } else {
                assert!(
                    decoder.len() >= usize::from(p.frames) * usize::from(p.angles),
                    "{}",
                    p.source
                );
            }

            for f in 0..decoder.len() {
                decoder.frame(f).unwrap();
            }
        }
    }
}
