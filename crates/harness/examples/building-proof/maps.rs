//! Isolated local map packages for building captures.
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn prepare(
    package: PathBuf,
    directory: &Path,
    bookmarks: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read(&package)?;
    let source: Value = serde_json::from_slice(&bytes)?;
    let name = package.file_name().ok_or("package name")?;
    let pages = package
        .parent()
        .ok_or("package directory")?
        .join("pages")
        .canonicalize()?;
    let request = &source["request"];
    let center = source["estimate"]["tiles_per_side"]
        .as_f64()
        .ok_or("tiles_per_side")?
        / 2.0;
    let parent = directory.parent().ok_or("output parent")?;
    fs::create_dir_all(parent)?;
    let output = parent
        .canonicalize()?
        .join(directory.file_name().ok_or("output name")?);
    if package.canonicalize()?.starts_with(&output)
        || pages.starts_with(&output)
        || output.starts_with(&pages)
    {
        return Err("source package and prepared maps must not overlap".into());
    }
    let prepared = tempfile::tempdir_in(parent)?;
    fs::write(prepared.path().join(name), bytes)?;
    copy_pages(&pages, &prepared.path().join("pages"))?;
    // Replace the whole prepared store, including pages no longer referenced.
    // Unlink a previous store symlink without following it into original data.
    if let Ok(metadata) = fs::symlink_metadata(directory) {
        if metadata.file_type().is_symlink() {
            fs::remove_file(directory)?;
        } else {
            fs::remove_dir_all(directory)?;
        }
    }
    fs::rename(prepared.path(), directory)?;
    let marks = json!({"maps":{"village":{
        "center_latitude_e7":request["center_latitude_e7"],
        "center_longitude_e7":request["center_longitude_e7"],
        "requested_side_meters":request["requested_side_meters"]
    }}, "bookmarks":[
        {"name":"tc-default","map":"village","x":center,"y":center,"zoom":1,"renderer":"webgl2"},
        {"name":"tc-close","map":"village","x":center,"y":center,"zoom":2,"renderer":"webgl2"}
    ]});
    fs::write(bookmarks, serde_json::to_vec_pretty(&marks)?)?;
    println!("prepared one unambiguous local map and camera bookmarks at {center},{center}");
    Ok(())
}

fn copy_pages(source: &Path, target: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let destination = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_pages(&entry.path(), &destination)?;
        } else if entry.file_type()?.is_file()
            && !destination.exists()
            && fs::hard_link(entry.path(), &destination).is_err()
        {
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}
