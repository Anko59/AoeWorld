//! `aoe-harness assets series-art`: web copies of the owner's generated paintings.
//!
//! The 24 series of PNG paintings stay outside Git. This writes compressed JPEG
//! copies and a manifest below ignored `local-assets/series-art/`, which the
//! dev server publishes at `/series-art/` when the directory exists. The menu
//! and loading screens read `manifest.json` and fall back to the bundled art.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const QUALITY: u8 = 84;
const MAX_SIDE: u32 = 8192;
const MAX_PNG_BYTES: u64 = 64 << 20;

#[derive(Deserialize)]
struct SourceSeries {
    id: String,
    name: String,
    images: Vec<SourceImage>,
}
#[derive(Deserialize)]
struct SourceImage {
    culture: String,
    image: String,
}
#[derive(Serialize)]
struct Manifest {
    version: u8,
    series: Vec<Series>,
}
#[derive(Serialize)]
struct Series {
    id: String,
    name: String,
    images: Vec<Image>,
}
#[derive(Serialize)]
struct Image {
    file: String,
    culture: String,
    width: u32,
    height: u32,
}

type Failure = Box<dyn std::error::Error>;

/// A name that stays one path component below its parent.
fn component(value: &str) -> Result<&str, Failure> {
    let plain = !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && !value.starts_with('.');
    plain
        .then_some(value)
        .ok_or_else(|| format!("unsafe series art name {value:?}").into())
}

/// Decodes a PNG to packed RGB8, compositing any transparency over black.
fn decode(path: &Path) -> Result<(Vec<u8>, u32, u32), Failure> {
    if fs::metadata(path)?.len() > MAX_PNG_BYTES {
        return Err(format!("{} is larger than the input bound", path.display()).into());
    }
    let mut decoder = png::Decoder::new(fs::File::open(path)?);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!("{} has unsupported dimensions", path.display()).into());
    }
    let mut buffer = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buffer)?;
    let pixels = &buffer[..frame.buffer_size()];
    let rgb = match frame.color_type {
        png::ColorType::Rgb => pixels.to_vec(),
        png::ColorType::Rgba => pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let a = u32::from(p[3]);
                [0, 1, 2].map(|i| (u32::from(p[i]) * a / 255) as u8)
            })
            .collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g]).collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| {
                let g = (u32::from(p[0]) * u32::from(p[1]) / 255) as u8;
                [g, g, g]
            })
            .collect(),
        png::ColorType::Indexed => return Err("indexed PNG was not expanded".into()),
    };
    Ok((rgb, width, height))
}

fn convert(source: &Path, target: &Path) -> Result<(u32, u32), Failure> {
    let (rgb, width, height) = decode(source)?;
    let temporary = target.with_extension("jpg.partial");
    let file = std::io::BufWriter::new(fs::File::create(&temporary)?);
    image::codecs::jpeg::JpegEncoder::new_with_quality(file, QUALITY).encode(
        &rgb,
        width,
        height,
        image::ExtendedColorType::Rgb8,
    )?;
    fs::rename(temporary, target)?;
    Ok((width, height))
}

pub(crate) fn run(source: &Path, output: &Path) -> Result<(), Failure> {
    if !output.starts_with("local-assets")
        || output
            .components()
            .any(|c| c == std::path::Component::ParentDir)
    {
        return Err("series art output must stay below ignored local-assets/".into());
    }
    let mut directories: Vec<PathBuf> = fs::read_dir(source)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("series.json").is_file())
        .collect();
    directories.sort();
    let mut manifest = Manifest {
        version: 1,
        series: Vec::new(),
    };
    let (mut images, mut bytes) = (0_u64, 0_u64);
    for directory in directories {
        let series: SourceSeries =
            serde_json::from_slice(&fs::read(directory.join("series.json"))?)?;
        let id = component(&series.id)?.to_owned();
        fs::create_dir_all(output.join(&id))?;
        let mut entry = Series {
            id: id.clone(),
            name: series.name,
            images: Vec::new(),
        };
        for image in series.images {
            let name = component(&image.image)?;
            let stem = name
                .strip_suffix(".png")
                .ok_or("series image is not a PNG")?;
            let relative = format!("{id}/{stem}.jpg");
            let target = output.join(&relative);
            let (width, height) = convert(&directory.join(name), &target)?;
            images += 1;
            bytes += fs::metadata(&target)?.len();
            entry.images.push(Image {
                file: relative,
                culture: image.culture,
                width,
                height,
            });
        }
        manifest.series.push(entry);
    }
    if manifest.series.is_empty() {
        return Err(format!("no series.json files below {}", source.display()).into());
    }
    let temporary = output.join("manifest.json.partial");
    fs::write(&temporary, serde_json::to_vec_pretty(&manifest)?)?;
    fs::rename(temporary, output.join("manifest.json"))?;
    println!(
        "wrote {images} images in {} series ({bytes} bytes) to {}",
        manifest.series.len(),
        output.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_cannot_leave_their_directory() {
        for bad in ["", "..", "../x", "a/b", ".hidden", "a b"] {
            assert!(component(bad).is_err(), "{bad}");
        }
        assert_eq!(component("01-franks.png").unwrap(), "01-franks.png");
    }

    #[test]
    fn writes_jpeg_copies_and_a_manifest() {
        let root = tempfile::tempdir().unwrap();
        let series = root.path().join("series/01-test");
        fs::create_dir_all(&series).unwrap();
        let mut file = fs::File::create(series.join("01-a.png")).unwrap();
        let mut encoder = png::Encoder::new(&mut file, 4, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[200; 4 * 2 * 4])
            .unwrap();
        fs::write(
            series.join("series.json"),
            br#"{"id":"01-test","name":"Test","images":[{"culture":"Franks","image":"01-a.png"}]}"#,
        )
        .unwrap();
        let (width, height) =
            convert(&series.join("01-a.png"), &root.path().join("o.jpg")).unwrap();
        assert_eq!((width, height), (4, 2));
        let jpeg = fs::read(root.path().join("o.jpg")).unwrap();
        assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
        assert!(run(&root.path().join("series"), Path::new("/tmp/elsewhere")).is_err());
    }
}
