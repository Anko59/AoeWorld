//! Bounded, byte-exact RGBA8 PNG decoding through the browser zlib stream.
use js_sys::{Array, Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{ReadableStream, ReadableStreamDefaultReader, WritableStreamDefaultWriter};

const SIDE: usize = 2048;
#[cfg(test)]
const PIXEL_BYTES: usize = SIDE * SIDE * 4;
const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;
const WRITE_CHUNK_BYTES: usize = 1024;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const ADAM7: [Pass; 7] = [
    Pass::new(0, 0, 8, 8),
    Pass::new(4, 0, 8, 8),
    Pass::new(0, 4, 4, 8),
    Pass::new(2, 0, 4, 4),
    Pass::new(0, 2, 2, 4),
    Pass::new(1, 0, 2, 2),
    Pass::new(0, 1, 1, 2),
];

#[derive(Clone, Copy)]
struct Pass {
    x: usize,
    y: usize,
    step_x: usize,
    step_y: usize,
}

impl Pass {
    const fn new(x: usize, y: usize, step_x: usize, step_y: usize) -> Self {
        Self {
            x,
            y,
            step_x,
            step_y,
        }
    }
}

struct CompressedPage {
    idat: Vec<u8>,
    interlaced: bool,
}

pub(super) async fn decode(bytes: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    let page = parse_png(&bytes)?;
    drop(bytes);
    let limit = filtered_size(SIDE, SIDE, page.interlaced)?;
    let scanlines = inflate(&page.idat, limit).await?;
    reconstruct(&scanlines, SIDE, SIDE, page.interlaced)
}

fn parse_png(bytes: &[u8]) -> Result<CompressedPage, JsValue> {
    validate_header(bytes)?;
    let mut idat = Vec::new();
    let (mut offset, mut saw_idat, mut idat_ended, mut saw_end) = (8, false, false, false);
    let mut interlaced = false;
    let mut saw_palette = false;
    while offset < bytes.len() {
        let chunk_head = bytes
            .get(offset..offset + 8)
            .ok_or_else(|| invalid("Truncated atlas PNG chunk"))?;
        let length = u32::from_be_bytes(chunk_head[..4].try_into().unwrap()) as usize;
        let chunk_end = offset
            .checked_add(length)
            .and_then(|end| end.checked_add(12))
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| invalid("Invalid atlas PNG chunk length"))?;
        let kind = &chunk_head[4..8];
        if !kind.iter().all(u8::is_ascii_alphabetic) || kind[2] & 0x20 != 0 {
            return Err(invalid("Invalid atlas PNG chunk type"));
        }
        let data = &bytes[offset + 8..chunk_end - 4];
        let stored_crc = u32::from_be_bytes(bytes[chunk_end - 4..chunk_end].try_into().unwrap());
        let mut crc = crc32fast::Hasher::new();
        crc.update(kind);
        crc.update(data);
        if crc.finalize() != stored_crc {
            return Err(invalid("Atlas PNG checksum mismatch"));
        }

        if offset == 8 {
            if kind != b"IHDR" || length != 13 {
                return Err(invalid("Invalid atlas PNG header"));
            }
            interlaced = data[12] == 1;
        } else if kind == b"IHDR" {
            return Err(invalid("Duplicate atlas PNG header"));
        }

        if saw_idat && kind != b"IDAT" {
            idat_ended = true;
        }
        match kind {
            b"IHDR" => {}
            b"PLTE"
                if !saw_idat
                    && !saw_palette
                    && data.len() >= 3
                    && data.len() <= 768
                    && data.len() % 3 == 0 =>
            {
                saw_palette = true;
            }
            b"tRNS" => return Err(invalid("Transparency chunk is invalid for RGBA atlas PNGs")),
            b"IDAT" if !idat_ended => {
                saw_idat = true;
                if idat.len().saturating_add(data.len()) > MAX_INPUT_BYTES {
                    return Err(invalid("Atlas PNG compressed data exceeds 32 MiB"));
                }
                idat.extend_from_slice(data);
            }
            b"IEND" if saw_idat && data.is_empty() => {
                saw_end = true;
                if chunk_end != bytes.len() {
                    return Err(invalid("Data follows atlas PNG end"));
                }
            }
            _ if kind[0] & 0x20 != 0 => {}
            _ => return Err(invalid("Unsupported atlas PNG chunk")),
        }
        offset = chunk_end;
        if saw_end {
            break;
        }
    }
    if !saw_idat || !saw_end {
        return Err(invalid("Incomplete atlas PNG"));
    }
    Ok(CompressedPage { idat, interlaced })
}

fn validate_header(bytes: &[u8]) -> Result<(), JsValue> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(invalid("Atlas PNG exceeds 32 MiB"));
    }
    let header = bytes
        .get(0..33)
        .ok_or_else(|| invalid("Invalid atlas PNG header"))?;
    if &header[..8] != PNG_SIGNATURE
        || u32::from_be_bytes(header[8..12].try_into().unwrap()) != 13
        || &header[12..16] != b"IHDR"
    {
        return Err(invalid("Invalid atlas PNG header"));
    }
    let width = u32::from_be_bytes(header[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(header[20..24].try_into().unwrap());
    if width != SIDE as u32 || height != SIDE as u32 {
        return Err(invalid("Unsupported atlas dimensions"));
    }
    if header[24] != 8 || header[25] != 6 || header[26] != 0 || header[27] != 0 || header[28] > 1 {
        return Err(invalid("Unsupported atlas pixel format"));
    }
    Ok(())
}

async fn inflate(compressed: &[u8], limit: usize) -> Result<Vec<u8>, JsValue> {
    let global = js_sys::global();
    let constructor =
        Reflect::get(&global, &"DecompressionStream".into())?.dyn_into::<Function>()?;
    let args = Array::new();
    args.push(&"deflate".into());
    let stream = Reflect::construct(&constructor, &args)?;
    let readable = Reflect::get(&stream, &"readable".into())?.dyn_into::<ReadableStream>()?;
    let writable =
        Reflect::get(&stream, &"writable".into())?.dyn_into::<web_sys::WritableStream>()?;
    let reader: ReadableStreamDefaultReader = readable.get_reader().dyn_into()?;
    let writer: WritableStreamDefaultWriter = writable.get_writer()?;
    let result = transfer_bounded(&reader, &writer, compressed, limit).await;
    if let Err(reason) = &result {
        let cancel = reader.cancel_with_reason(reason);
        let abort = writer.abort_with_reason(reason);
        let _ = JsFuture::from(cancel).await;
        let _ = JsFuture::from(abort).await;
    }
    result
}

async fn transfer_bounded(
    reader: &ReadableStreamDefaultReader,
    writer: &WritableStreamDefaultWriter,
    compressed: &[u8],
    limit: usize,
) -> Result<Vec<u8>, JsValue> {
    let mut output = Vec::with_capacity(limit);
    let mut pending_read = reader.read();
    for input_chunk in compressed.chunks(WRITE_CHUNK_BYTES) {
        let input = Uint8Array::from(input_chunk);
        let write = writer.write_with_chunk(input.as_ref());
        loop {
            let (read_won, value) = race_read_write(&pending_read, &write).await?;
            if !read_won {
                break;
            }
            if consume_read(value, &mut output, limit)? {
                return Err(invalid("Truncated atlas PNG compressed stream"));
            }
            pending_read = reader.read();
        }
    }

    let close = writer.close();
    let mut close_done = false;
    loop {
        if close_done {
            let value = JsFuture::from(pending_read).await?;
            if consume_read(value, &mut output, limit)? {
                break;
            }
            pending_read = reader.read();
        } else {
            let (read_won, value) = race_read_write(&pending_read, &close).await?;
            if read_won {
                if consume_read(value, &mut output, limit)? {
                    JsFuture::from(close.clone()).await?;
                    break;
                }
                pending_read = reader.read();
            } else {
                close_done = true;
            }
        }
    }
    Ok(output)
}

async fn race_read_write(read: &Promise, write: &Promise) -> Result<(bool, JsValue), JsValue> {
    let promises = Array::new();
    promises.push(read.as_ref());
    promises.push(write.as_ref());
    let result = JsFuture::from(Promise::race(promises.as_ref())).await?;
    Ok((!result.is_undefined(), result))
}

fn consume_read(record: JsValue, output: &mut Vec<u8>, limit: usize) -> Result<bool, JsValue> {
    if Reflect::get(&record, &"done".into())?
        .as_bool()
        .unwrap_or(false)
    {
        return Ok(true);
    }
    let chunk = Reflect::get(&record, &"value".into())?.dyn_into::<Uint8Array>()?;
    let length = chunk.length() as usize;
    if output.len().saturating_add(length) > limit {
        return Err(invalid("Atlas PNG expands beyond its dimension limit"));
    }
    let mut decoded = vec![0; length];
    chunk.copy_to(&mut decoded);
    output.extend_from_slice(&decoded);
    Ok(false)
}

fn filtered_size(width: usize, height: usize, interlaced: bool) -> Result<usize, JsValue> {
    const SINGLE_PASS: [Pass; 1] = [Pass::new(0, 0, 1, 1)];
    let passes: &[Pass] = if interlaced { &ADAM7 } else { &SINGLE_PASS };
    passes.iter().try_fold(0_usize, |sum, pass| {
        let pass_width = pass_size(width, pass.x, pass.step_x);
        let pass_height = pass_size(height, pass.y, pass.step_y);
        if pass_width == 0 || pass_height == 0 {
            return Ok(sum);
        }
        let row_bytes = pass_width
            .checked_mul(4)
            .and_then(|bytes| bytes.checked_add(1))
            .ok_or_else(|| invalid("Atlas PNG dimensions overflow"))?;
        sum.checked_add(row_bytes.saturating_mul(pass_height))
            .ok_or_else(|| invalid("Atlas PNG dimensions overflow"))
    })
}

fn pass_size(length: usize, start: usize, step: usize) -> usize {
    if length <= start {
        0
    } else {
        (length - start).div_ceil(step)
    }
}

fn reconstruct(
    scanlines: &[u8],
    width: usize,
    height: usize,
    interlaced: bool,
) -> Result<Vec<u8>, JsValue> {
    const SINGLE_PASS: [Pass; 1] = [Pass::new(0, 0, 1, 1)];
    let expected = filtered_size(width, height, interlaced)?;
    if scanlines.len() != expected {
        return Err(invalid("Invalid atlas PNG scanline data"));
    }
    let mut pixels = vec![0; width * height * 4];
    let passes: &[Pass] = if interlaced { &ADAM7 } else { &SINGLE_PASS };
    let mut source = 0;
    for pass in passes {
        let pass_width = pass_size(width, pass.x, pass.step_x);
        let pass_height = pass_size(height, pass.y, pass.step_y);
        if pass_width == 0 || pass_height == 0 {
            continue;
        }
        let row_bytes = pass_width * 4;
        let mut previous = vec![0; row_bytes];
        let mut row = vec![0; row_bytes];
        for row_index in 0..pass_height {
            let filter = scanlines[source];
            source += 1;
            row.copy_from_slice(&scanlines[source..source + row_bytes]);
            source += row_bytes;
            unfilter(&mut row, &previous, filter)?;
            let target_y = pass.y + row_index * pass.step_y;
            for column in 0..pass_width {
                let target_x = pass.x + column * pass.step_x;
                let from = column * 4;
                let to = (target_y * width + target_x) * 4;
                pixels[to..to + 4].copy_from_slice(&row[from..from + 4]);
            }
            std::mem::swap(&mut previous, &mut row);
        }
    }
    Ok(pixels)
}

fn unfilter(row: &mut [u8], previous: &[u8], filter: u8) -> Result<(), JsValue> {
    if filter > 4 {
        return Err(invalid("Unsupported atlas PNG filter"));
    }
    for index in 0..row.len() {
        let left = if index >= 4 { row[index - 4] } else { 0 };
        let above = previous[index];
        let upper_left = if index >= 4 { previous[index - 4] } else { 0 };
        let predictor = match filter {
            0 => 0,
            1 => left,
            2 => above,
            3 => ((u16::from(left) + u16::from(above)) / 2) as u8,
            _ => paeth(left, above, upper_left),
        };
        row[index] = row[index].wrapping_add(predictor);
    }
    Ok(())
}

fn paeth(left: u8, above: u8, upper_left: u8) -> u8 {
    let (left, above, upper_left) = (i32::from(left), i32::from(above), i32::from(upper_left));
    let estimate = left + above - upper_left;
    let (left_distance, above_distance, corner_distance) = (
        (estimate - left).abs(),
        (estimate - above).abs(),
        (estimate - upper_left).abs(),
    );
    if left_distance <= above_distance && left_distance <= corner_distance {
        left as u8
    } else if above_distance <= corner_distance {
        above as u8
    } else {
        upper_left as u8
    }
}

fn invalid(message: &str) -> JsValue {
    JsValue::from_str(message)
}

#[cfg(test)]
mod tests;
