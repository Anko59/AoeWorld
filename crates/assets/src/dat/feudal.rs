/// Feudal building and age-up families. An absent trial unit is explicit.
pub const FEUDAL: &[(&str, i32)] = &[
    ("Archery Range", 87),
    ("Stable", 101),
    ("Blacksmith", 103),
    ("Market", 84),
    ("Watch Tower", 79),
    ("Palisade Gate", 792),
    ("Town Center (Feudal)", 71),
    ("House (Feudal)", 463),
    ("Mill (Feudal)", 129),
    ("Lumber Camp (Feudal)", 563),
    ("Mining Camp (Feudal)", 585),
    ("Barracks (Feudal)", 498),
    ("Dock (Feudal)", 133),
];

/// Military survey records are actors/projectiles, not building compositions.
pub const FEUDAL_MILITARY: &[(&str, i32)] = &[
    ("Skirmisher", 7),
    ("Spearman", 93),
    ("Man-at-Arms", 75),
    ("Arrow", 363),
    ("Javelin", 365),
    ("Galley", 539),
    ("War Galley", 21),
    ("Fire Ship", 529),
    ("Demolition Ship", 527),
    ("Transport Ship", 545),
];

/// Core generic Castle actors and buildings; identifiers are DAT unit ids.
pub const CASTLE: &[(&str, i32)] = &[
    ("Castle", 82),
    ("Monastery", 104),
    ("University", 209),
    ("Siege Workshop", 49),
    ("Long Swordsman", 77),
    ("Pikeman", 358),
    ("Crossbowman", 24),
    ("Elite Skirmisher", 6),
    ("Knight", 38),
    ("Light Cavalry", 546),
    ("Monk", 125),
    ("Mangonel", 280),
    ("Battering Ram", 35),
];
