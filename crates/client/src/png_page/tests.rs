use super::*;
use std::io::{Read, Write};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

fn importer_page(pixels: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, SIDE as u32, SIDE as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
        writer.finish().unwrap();
    }
    encoded
}

fn write_chunk(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(data);
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    crc.update(data);
    output.extend_from_slice(&crc.finalize().to_be_bytes());
}

fn idat_data(encoded: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    let mut offset = 8;
    while offset < encoded.len() {
        let length = u32::from_be_bytes(encoded[offset..offset + 4].try_into().unwrap()) as usize;
        let kind = &encoded[offset + 4..offset + 8];
        if kind == b"IDAT" {
            data.extend_from_slice(&encoded[offset + 8..offset + 8 + length]);
        }
        offset += length + 12;
    }
    data
}

fn encoded_page(interlaced: bool, parts: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut output = PNG_SIGNATURE.to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&(SIDE as u32).to_be_bytes());
    header.extend_from_slice(&(SIDE as u32).to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, u8::from(interlaced)]);
    write_chunk(&mut output, b"IHDR", &header);
    for (kind, data) in parts {
        write_chunk(&mut output, kind, data);
    }
    write_chunk(&mut output, b"IEND", &[]);
    output
}

fn adam7_page(pixels: &[u8]) -> Vec<u8> {
    let mut scanlines = Vec::with_capacity(filtered_size(SIDE, SIDE, true).unwrap());
    for pass in ADAM7 {
        let pass_width = pass_size(SIDE, pass.x, pass.step_x);
        let pass_height = pass_size(SIDE, pass.y, pass.step_y);
        for row in 0..pass_height {
            scanlines.push(0);
            for column in 0..pass_width {
                let x = pass.x + column * pass.step_x;
                let y = pass.y + row * pass.step_y;
                let start = (y * SIDE + x) * 4;
                scanlines.extend_from_slice(&pixels[start..start + 4]);
            }
        }
    }
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&scanlines).unwrap();
    let compressed = encoder.finish().unwrap();
    encoded_page(true, &[(b"IDAT", &compressed)])
}

#[wasm_bindgen_test]
async fn importer_style_png_preserves_rgba_crc_chunks_and_decompression_bounds() {
    let mut expected = vec![0; PIXEL_BYTES];
    let samples = [
        [17, 83, 201, 255],
        [0, 0, 0, 0],
        [0, 0, 0, 128],
        [101, 17, 203, 128],
    ];
    for (index, sample) in samples.iter().enumerate() {
        expected[index * 4..index * 4 + 4].copy_from_slice(sample);
    }
    let encoded = importer_page(&expected);
    assert_eq!(decode(encoded.clone()).await.unwrap(), expected);

    let compressed = idat_data(&encoded);
    let split_at = compressed.len() / 2;
    let first = encoded_page(
        false,
        &[
            (b"IDAT", &compressed[..split_at]),
            (b"IDAT", &compressed[split_at..]),
        ],
    );
    assert_eq!(decode(first).await.unwrap(), expected);

    let separated = encoded_page(
        false,
        &[
            (b"IDAT", &compressed[..split_at]),
            (b"tEXt", b"Comment\0test"),
            (b"IDAT", &compressed[split_at..]),
        ],
    );
    assert!(decode(separated).await.is_err());
    assert!(
        decode(encoded_page(
            false,
            &[(b"tRNS", b"\0\0"), (b"IDAT", &compressed)]
        ))
        .await
        .is_err()
    );
    assert!(
        decode(encoded_page(
            false,
            &[(b"abca", b"x"), (b"IDAT", &compressed)]
        ))
        .await
        .is_err()
    );

    let mut bad_crc = encoded.clone();
    bad_crc[29] ^= 1;
    assert!(decode(bad_crc).await.is_err());
    let mut bad_length = encoded.clone();
    bad_length[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(decode(bad_length).await.is_err());
    assert!(decode(encoded[..40].to_vec()).await.is_err());
    assert!(inflate(&compressed, 1).await.is_err());

    let truncated = encoded_page(false, &[(b"IDAT", &compressed[..compressed.len() - 3])]);
    assert!(decode(truncated).await.is_err());
    let mut bad_deflate = compressed.clone();
    bad_deflate[0] ^= 0xff;
    assert!(
        decode(encoded_page(false, &[(b"IDAT", &bad_deflate)]))
            .await
            .is_err()
    );
}

#[wasm_bindgen_test]
async fn adam7_png_with_partial_alpha_decodes_to_original_pixel_coordinates() {
    let mut expected = vec![0; PIXEL_BYTES];
    let points = [
        (0, 0, [101, 17, 203, 128]),
        (1, 0, [19, 211, 73, 64]),
        (0, 1, [7, 31, 149, 192]),
        (SIDE - 1, SIDE - 1, [235, 99, 13, 255]),
    ];
    for (x, y, rgba) in points {
        let index = (y * SIDE + x) * 4;
        expected[index..index + 4].copy_from_slice(&rgba);
    }
    assert_eq!(decode(adam7_page(&expected)).await.unwrap(), expected);
}

#[wasm_bindgen_test]
fn filters_zero_through_four_reconstruct_rgba_rows() {
    for filter in [
        png::FilterType::NoFilter,
        png::FilterType::Sub,
        png::FilterType::Up,
        png::FilterType::Avg,
        png::FilterType::Paeth,
    ] {
        let pixels: Vec<u8> = (0..3 * 5 * 4)
            .map(|index| (index * 29 + index / 3 * 11) as u8)
            .collect();
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, 3, 5);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(png::Compression::Default);
            encoder.set_filter(filter);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&pixels).unwrap();
            writer.finish().unwrap();
        }
        let compressed = idat_data(&encoded);
        let mut scanlines = Vec::new();
        flate2::read::ZlibDecoder::new(compressed.as_slice())
            .read_to_end(&mut scanlines)
            .unwrap();
        let filter_bytes: Vec<_> = scanlines.chunks_exact(13).map(|row| row[0]).collect();
        assert_eq!(filter_bytes, vec![filter as u8; 5], "filter {filter:?}");
        assert_eq!(reconstruct(&scanlines, 3, 5, false).unwrap(), pixels);
    }
}

#[wasm_bindgen_test]
fn adam7_rows_scatter_to_the_original_image_coordinates() {
    let (width, height) = (4, 4);
    let pixels: Vec<u8> = (0..width * height * 4).map(|value| value as u8).collect();
    let mut scanlines = Vec::new();
    for pass in ADAM7 {
        let pass_width = pass_size(width, pass.x, pass.step_x);
        let pass_height = pass_size(height, pass.y, pass.step_y);
        if pass_width == 0 || pass_height == 0 {
            continue;
        }
        for row in 0..pass_height {
            scanlines.push(0);
            for column in 0..pass_width {
                let x = pass.x + column * pass.step_x;
                let y = pass.y + row * pass.step_y;
                let start = (y * width + x) * 4;
                scanlines.extend_from_slice(&pixels[start..start + 4]);
            }
        }
    }
    assert_eq!(
        reconstruct(&scanlines, width, height, true).unwrap(),
        pixels
    );
}

#[wasm_bindgen_test]
fn header_checks_dimensions_and_format_before_stream_decode() {
    let mut header = [0; 33];
    header[..8].copy_from_slice(PNG_SIGNATURE);
    header[8..12].copy_from_slice(&13_u32.to_be_bytes());
    header[12..16].copy_from_slice(b"IHDR");
    header[16..20].copy_from_slice(&(SIDE as u32).to_be_bytes());
    header[20..24].copy_from_slice(&(SIDE as u32).to_be_bytes());
    header[24..29].copy_from_slice(&[8, 6, 0, 0, 0]);
    assert!(validate_header(&header).is_ok());
    header[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(validate_header(&header).is_err());
    header[16..20].copy_from_slice(&(SIDE as u32).to_be_bytes());
    header[25] = 2;
    assert!(validate_header(&header).is_err());
}
