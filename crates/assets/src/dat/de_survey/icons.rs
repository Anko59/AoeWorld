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
    // Naval Warfare Overhaul research: language name ids from the DE data.
    (906, 507005),
    (65, 7314),
    (374, 7372),
    (375, 7373),
    (373, 7377),
    (908, 507003),
    (907, 507004),
    (909, 507006),
    (910, 507007),
    (35, 7035),
    (905, 507002),
    (244, 7242),
    (376, 7375),
    // Gunpowder, mounted archers, camels and siege upgrades.
    (47, 7047),
    (218, 7218),
    (236, 7235),
    (239, 7244),
    (320, 7323),
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
    let wanted: std::collections::BTreeMap<_, _> = TECHS
        .iter()
        .chain(ADDITIONAL)
        .map(|&(id, language)| (language, id))
        .collect();
    let mut icons = std::collections::BTreeMap::new();
    // The icon can exist even for a DE entry whose effect was disabled.
    // Locate the validated technology header from the debug name marker.
    for (at, marker) in bytes.windows(4).enumerate() {
        if marker[..2] != [0x60, 0x0a] || marker[3] != 0 || at < 55 {
            continue;
        }
        let mut identity = Reader::new(bytes, at - 22);
        let language = identity.i32()?;
        let Some(&id) = wanted.get(&language) else {
            continue;
        };
        if identity.i32()? != language + 1000 {
            continue;
        }
        if let Ok(tech) = technology_at(bytes, id, at - 55)
            && tech.icon >= 0
            && icons
                .insert(id, tech.icon)
                .is_some_and(|previous| previous != tech.icon)
        {
            return Err(crate::invalid(
                "DE icons",
                tech.offset,
                "ambiguous research icon",
            ));
        }
    }
    if icons.len() != wanted.len() {
        let missing: Vec<_> = wanted
            .values()
            .filter(|id| !icons.contains_key(id))
            .collect();
        return Err(crate::invalid(
            "DE icons",
            0,
            format!("missing research icons: {missing:?}"),
        ));
    }
    Ok(icons.into_iter().collect())
}

pub(super) fn technology_at(bytes: &[u8], id: i16, start: usize) -> Result<Technology, Error> {
    let mut r = Reader::new(bytes, start);
    let requires = (0..6).map(|_| r.i16()).collect::<Result<Vec<_>, _>>()?;
    let cost = (0..3)
        .map(|_| Ok((r.i16()?, r.i16()?, r.u8()?)))
        .collect::<Result<Vec<_>, Error>>()?;
    r.skip(6 + 8)?;
    let effect = r.i16()?;
    r.skip(2)?;
    let icon = r.i16()?;
    r.skip(8)?;
    let name = debug(&mut r)?;
    r.skip(1)?;
    let n = count(&mut r, 16, "research locations")?;
    let locations = (0..n)
        .map(|_| Ok((r.i16()?, r.i16()?, r.u8()?, r.i32()?)))
        .collect::<Result<Vec<_>, Error>>()?;
    if locations.is_empty() || effect < -1 {
        return Err(r.fail("invalid research record"));
    }
    Ok(Technology {
        id,
        offset: start,
        name,
        requires,
        cost,
        effect,
        icon,
        locations,
    })
}
