//! Verify the served image bytes before binding them to the release manifest.
use super::{Manifest, Result, Runtime, collect_files};
use std::{fs, path::Path};

pub(super) fn verify_shaders(web: &Path) -> Result<()> {
    for (name, expected) in crate::assets::browser_shaders() {
        let path = web.join(format!("pkg/{name}.wgsl"));
        let actual = fs::read(&path)
            .map_err(|error| format!("release runtime asset {}: {error}", path.display()))?;
        if actual != expected.as_bytes() {
            return Err(format!("release runtime asset mismatch: {}", path.display()).into());
        }
    }
    Ok(())
}

pub(super) fn verify_images<R: Runtime>(
    runtime: &R,
    manifest: &Manifest,
    bundle: &Path,
) -> Result<()> {
    for (image, web) in [
        (&manifest.server_image_id, "/app/web/."),
        (&manifest.browser_image_id, "/usr/share/nginx/html/."),
    ] {
        let exported = tempfile::tempdir()?;
        let container = runtime.output("docker", &["create", image])?;
        let copied = runtime.checked(
            "docker",
            &[
                "cp",
                &format!("{container}:{web}"),
                exported
                    .path()
                    .to_str()
                    .ok_or("non-UTF-8 image export path")?,
            ],
        );
        let removed = runtime.checked("docker", &["rm", "-f", &container]);
        copied?;
        removed?;
        verify_shaders(exported.path())?;
        let mut files = Vec::new();
        collect_files(bundle, bundle, &mut files)?;
        for relative in files {
            let actual = fs::read(exported.path().join(&relative)).map_err(|error| {
                format!(
                    "release image {image} missing {}: {error}",
                    relative.display()
                )
            })?;
            if actual != fs::read(bundle.join(&relative))? {
                return Err(format!(
                    "release image {image} content differs from bundle manifest: {}",
                    relative.display()
                )
                .into());
            }
        }
    }
    Ok(())
}
