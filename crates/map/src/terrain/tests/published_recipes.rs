//! Golden semantic chunks captured before the landscape-v2 implementation.
//! Updating the latest recipe must not change recipes 3 through 8.
use super::super::MapChunkGenerator;

#[test]
fn published_recipe_chunks_keep_their_original_semantics() {
    let mut actual = Vec::new();
    for recipe in 3..=8 {
        let mut digest = blake3::Hasher::new();
        for seed in [1, 17, 991] {
            let generator =
                MapChunkGenerator::new([23; 32], seed, 512).with_elevation_sampling_recipe(recipe);
            for (x, y) in [(0, 0), (5, 6), (7, 7), (15, 15)] {
                let chunk = generator.chunk(x, y).expect("published chunk");
                digest.update(&serde_json::to_vec(&chunk).expect("semantic chunk"));
            }
        }
        actual.push(digest.finalize().to_hex().to_string());
    }
    assert_eq!(
        actual,
        [
            "50636aa1726f48ae86ba07a1cab7536de9095925bec5a54454b4fd7b1adb751e",
            "50636aa1726f48ae86ba07a1cab7536de9095925bec5a54454b4fd7b1adb751e",
            "184a8fb7b42dae6b675baba8c7bb7cb553f37f0e78aab8d7439c6bb157dd6aaa",
            "184a8fb7b42dae6b675baba8c7bb7cb553f37f0e78aab8d7439c6bb157dd6aaa",
            "53154537c5000845a4d5d9b887ed01059bc19d4be9d71007bcda6ff1d5f1507d",
            "8aaf29610a2e8f5df9242012889bd73dd8a9f52b6a36c6ca051ce6df1711e8ec",
        ]
    );
}
