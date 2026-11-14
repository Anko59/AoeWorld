//! Current interface research IDs and their native language identities.
use super::*;

const ADDITIONAL: &[(i16, i32)] = &[
    (22, 7022),
    (34, 7034),
    (103, 7103),
    (217, 7217),
    (264, 7260),
    (429, 7414),
    (237, 7233),
    (209, 7207),
    (265, 7259),
    (428, 7417),
    (96, 7096),
    (255, 7263),
    (257, 7262),
    (75, 7075),
    (77, 7077),
    (80, 7080),
    (201, 7151),
    (219, 7216),
    (12, 7012),
    (221, 7231),
    (279, 7277),
    // Generic technologies of content track B (DAT ids, language name ids).
    (215, 7210),
    (602, 7258),
    (875, 7450),
    (437, 7411),
    (436, 7415),
    (435, 7409),
    (39, 7039),
    (51, 7051),
    (377, 7378),
    (380, 7374),
    (608, 7278),
    (194, 7163),
    (140, 7186),
    (63, 7063),
    (46, 7462),
    (45, 7045),
    (252, 7249),
    (231, 7221),
    (441, 7435),
    (438, 7416),
    (233, 7220),
    (230, 7222),
    (319, 7316),
    (316, 7315),
    (439, 7412),
    (315, 7319),
    (379, 7376),
    (321, 7322),
    (408, 7408),
    (48, 7410),
    (15, 7015),
    (17, 7017),
    (23, 7023),
    (54, 7054),
];

/// Resolve each current UI technology's icon from the supported native DAT.
/// IDs identify gameplay research; language pairs locate validated records.
pub fn technology_icons(bytes: &[u8]) -> Result<Vec<(i16, i16)>, Error> {
    if bytes.get(..8) != Some(b"VER 8.9\0") {
        return Err(crate::invalid("DE icons", 0, "unsupported DAT version"));
    }
    TECHS
        .iter()
        .chain(ADDITIONAL)
        .map(|&(id, language)| {
            let tech = technology(bytes, id, language)?;
            if tech.icon < 0 {
                return Err(crate::invalid(
                    "DE icons",
                    tech.offset,
                    "missing research icon",
                ));
            }
            Ok((id, tech.icon))
        })
        .collect()
}
