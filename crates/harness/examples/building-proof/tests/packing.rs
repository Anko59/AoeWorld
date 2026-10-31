#[path = "../packing.rs"]
mod packing;
use aoe_assets::{catalog::runtime, pack::Manifest};

fn fixture() -> Manifest {
    let mut value = serde_json::json!({
        "version":1, "converter":"synthetic", "input_hash":"synthetic",
        "building_data":"00", "frames":[], "buildings":[], "chunks":[], "terrains":[],
        "pages":[{"color":"fixture.png","color_hash":"","player":"fixture.png",
            "player_hash":"","shadow":"fixture.png","shadow_hash":"",
            "outline":"fixture.png","outline_hash":"","width":2048,"height":2048}]
    });
    for source in runtime::REQUIRED {
        for frame in 0..source.frames {
            value["frames"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({
                    "source":source.manifest_source(),"source_hash":"","frame":frame,
                    "page":0,"x":0,"y":0,"width":2,"height":2,"anchor_x":0,"anchor_y":0
                }));
        }
    }
    let part = |slp, frames, angles| {
        serde_json::json!({
            "graphic_id":slp,"slp_id":slp,"source":format!("fixture:{slp}"),
            "available":true,"frames":frames,"angles":angles,"primary":true,
            "replay_delay":0.0,"frame_rate":0.0,"offset":[0,0],"misplacement":[0.0,0.0],"layer":20
        })
    };
    value["buildings"] = serde_json::json!([{
        "unit_id":70,"footprint":[2.0,2.0],"foundation_terrain":2,
        "foundation_slp":null,"foundation_size":[0,0],"damage":[],"parts":[part(2223,3,1)],
        "states":[],"construction":(236..=239).map(|slp| part(slp,1,3)).collect::<Vec<_>>()
    }]);
    for slp in [2223, 236, 237, 238, 239] {
        for frame in 0..3 {
            let mut record = value["frames"][0].clone();
            record["source"] = format!("fixture:{slp}").into();
            record["frame"] = frame.into();
            value["frames"].as_array_mut().unwrap().push(record);
        }
    }
    serde_json::from_value(value).unwrap()
}

#[test]
fn building_proof_checks_every_static_variant_and_construction_angle() {
    let manifest = fixture();
    let directory = tempfile::tempdir().unwrap();
    let check = |manifest: &Manifest| {
        std::fs::write(
            directory.path().join("manifest.json"),
            serde_json::to_vec(manifest).unwrap(),
        )
        .unwrap();
        packing::check(directory.path().to_path_buf())
    };
    assert!(check(&manifest).is_ok());
    for index in manifest.frames.len() - 15..manifest.frames.len() {
        let mut bad = manifest.clone();
        bad.frames.remove(index);
        assert!(check(&bad).is_err(), "missing runtime frame {index}");
    }
    for index in manifest.frames.len() - 15..manifest.frames.len() {
        let mut bad = manifest.clone();
        bad.frames[index].x = 2048;
        assert!(check(&bad).is_err(), "out-of-bounds runtime frame {index}");
        let mut bad = manifest.clone();
        // Fits the source page but cannot fit the runtime's reserved border.
        // Merely validating an omitted record must not count as packing it.
        bad.frames[index].width = 2045;
        assert!(check(&bad).is_err(), "unpacked runtime frame {index}");
        let mut bad = manifest.clone();
        bad.frames.push(bad.frames[index].clone());
        assert!(check(&bad).is_err(), "duplicate runtime frame {index}");
    }
}

#[test]
fn building_proof_includes_present_actor_animations() {
    let mut manifest = fixture();
    let source = aoe_assets::catalog::units::SPRITES[0];
    for frame in 0..source.frames() {
        let mut record = manifest.frames[0].clone();
        record.source = source.manifest_source();
        record.frame = frame;
        manifest.frames.push(record);
    }
    let directory = tempfile::tempdir().unwrap();
    let check = |manifest: &Manifest| {
        std::fs::write(
            directory.path().join("manifest.json"),
            serde_json::to_vec(manifest).unwrap(),
        )
        .unwrap();
        packing::check(directory.path().to_path_buf())
    };
    assert!(check(&manifest).is_ok());
    manifest.frames.last_mut().unwrap().width = 2045;
    assert!(
        check(&manifest).is_err(),
        "actor must occupy the shared atlas"
    );
    manifest.frames.pop();
    assert!(
        check(&manifest).is_err(),
        "present incomplete actor must fail"
    );
}
