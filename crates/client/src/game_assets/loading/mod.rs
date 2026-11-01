//! Startup selection and static uploads beside the native animation cache.
use super::*;
mod art;
mod prepare;
mod terrain;

#[inline(never)]
pub(crate) async fn load(
    sink: &mut dyn AtlasSink,
) -> Result<(GameArt, [u8; 3], Option<NativeFrames>), JsValue> {
    let team = team_colour();
    let bytes = fetch("/asset-pack/manifest.json").await?;
    let mut manifest = decode_manifest(&bytes)?;
    manifest.buildings =
        recipes::decode(&manifest.building_data, &manifest.frames).map_err(error)?;
    building_sprites::overlay(&mut manifest).await?;
    // The DTO owns its used strings; release raw JSON before PNG decode awaits.
    drop(bytes);
    let prepared = prepare::prepare(manifest, team)?;
    streaming::Input {
        pages: prepared.pages,
        manifest: &prepared.manifest,
        selected: &prepared.selected,
        placements: &prepared.placements,
        maskable: &prepared.maskable,
        actor_ranges: &prepared.actor_ranges,
        art: &prepared.art,
        team,
    }
    .upload(sink)
    .await?;
    Ok((prepared.art, team, prepared.native))
}
