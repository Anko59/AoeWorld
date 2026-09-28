use super::*;

#[test]
fn public_tile_names_match_the_aws_one_degree_catalog() {
    assert_eq!(
        tile_prefix(48, 2, "30"),
        "Copernicus_DSM_COG_30_N48_00_E002_00_DEM"
    );
    assert_eq!(
        tile_prefix(-1, -7, "10"),
        "Copernicus_DSM_COG_10_S01_00_W007_00_DEM"
    );
}
