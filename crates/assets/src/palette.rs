//! JASC-PAL text palettes.
use crate::{Error, invalid};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Palette(pub Vec<[u8; 3]>);

pub fn parse(bytes: &[u8]) -> Result<Palette, Error> {
    if bytes.len() > 32 * 1024 {
        return Err(invalid("palette", 0, "palette exceeds 32 KiB"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("palette", 0, "non-UTF8 palette"))?;
    let mut lines = text.lines();
    if lines.next() != Some("JASC-PAL") || lines.next() != Some("0100") {
        return Err(invalid("palette", 0, "expected JASC-PAL 0100"));
    }
    let count: usize = lines
        .next()
        .ok_or_else(|| invalid("palette", 0, "missing count"))?
        .trim()
        .parse()
        .map_err(|_| invalid("palette", 0, "invalid count"))?;
    if count == 0 || count > 256 {
        return Err(invalid("palette", 0, "count must be 1..=256"));
    }
    let mut colors = Vec::with_capacity(count);
    for index in 0..count {
        let line = lines
            .next()
            .ok_or_else(|| invalid("palette", index + 3, "missing color"))?;
        let values: Vec<_> = line.split_whitespace().collect();
        if values.len() != 3 {
            return Err(invalid("palette", index + 3, "expected RGB triple"));
        }
        let mut rgb = [0; 3];
        for (channel, value) in values.iter().enumerate() {
            rgb[channel] = value
                .parse()
                .map_err(|_| invalid("palette", index + 3, "invalid RGB channel"))?;
        }
        colors.push(rgb);
    }
    if lines.any(|line| !line.trim().is_empty()) {
        return Err(invalid("palette", count + 3, "extra color data"));
    }
    Ok(Palette(colors))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_fixture() {
        assert_eq!(
            parse(b"JASC-PAL\r\n0100\r\n2\r\n1 2 3\r\n255 0 80\r\n").unwrap(),
            Palette(vec![[1, 2, 3], [255, 0, 80]])
        );
        assert!(parse(b"JASC-PAL\n0100\n1\n256 0 0\n").is_err());
    }

    #[test]
    fn rejects_truncated_and_ambiguous_palettes() {
        for data in [
            &b""[..],
            &b"JASC-PAL\n0001\n1\n1 2 3\n"[..],
            &b"JASC-PAL\n0100\n"[..],
            &b"JASC-PAL\n0100\nnope\n"[..],
            &b"JASC-PAL\n0100\n0\n"[..],
            &b"JASC-PAL\n0100\n1\n"[..],
            &b"JASC-PAL\n0100\n1\n1 2\n"[..],
            &b"JASC-PAL\n0100\n1\n1 x 3\n"[..],
            &b"JASC-PAL\n0100\n1\n1 2 3\n4 5 6\n"[..],
            &b"JASC-PAL\n0100\n1\n\xFF 2 3\n"[..],
        ] {
            assert!(parse(data).is_err(), "accepted malformed palette: {data:?}");
        }
    }
}
