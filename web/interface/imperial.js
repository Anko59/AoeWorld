// Semantic metadata only. Prices, requirements and availability come from Rust.
// Order follows the protocol's UnitKind and Tech enums after General / WarGalley.
export const imperialUnits=['TwoHandedSwordsman','Champion','Halberdier','Arbalester','Cavalier','Paladin','Hussar','CappedRam','SiegeRam','Onager'];
export const imperialUnitNames=['Two-Handed Swordsman','Champion','Halberdier','Arbalester','Cavalier','Paladin','Hussar','Capped Ram','Siege Ram','Onager'];
// [id, label, building kind index]
export const imperialTechs=[
  ['imperial-age','Imperial Age · 1000 food · 800 gold',0],
  ['two-handed-swordsman','Two-Handed Swordsman · 200 food · 100 gold',6],
  ['champion','Champion · 650 food · 350 gold',6],
  ['halberdier','Halberdier · 300 food · 600 gold',6],
  ['arbalester','Arbalester · 450 food · 350 gold',10],
  ['cavalier','Cavalier · 300 food · 300 gold',11],
  ['paladin','Paladin · 1300 food · 750 gold',11],
  ['hussar','Hussar · 500 food · 600 gold',11],
  ['capped-ram','Capped Ram · 300 food',19],
  ['siege-ram','Siege Ram · 1000 food',19],
  ['onager','Onager · 800 food · 500 gold',19],
  ['blast-furnace','Blast Furnace · 275 food · 225 gold',12],
  ['plate-mail-armor','Plate Mail Armor · 300 food · 150 gold',12],
  ['plate-barding-armor','Plate Barding Armor · 350 food · 200 gold',12],
  ['bracer','Bracer · 300 food · 200 gold',12],
  ['ring-archer-armor','Ring Archer Armor · 250 food · 250 gold',12],
  ['crop-rotation','Crop Rotation · 250 food · 250 wood',4],
  ['two-man-saw','Two-Man Saw · 300 food · 200 wood',2],
  ['stone-shaft-mining','Stone Shaft Mining · 175 food · 75 wood',3],
];
// [id, label, cost, building kind index, UnitKind index]
export const imperialTraining=[
  ['halberdier','Halberdier','35 food · 25 wood',6,26],
  ['arbalester','Arbalester','25 wood · 45 gold',10,27],
  ['cavalier','Cavalier','60 food · 75 gold',11,28],
  ['paladin','Paladin','60 food · 75 gold',11,29],
  ['hussar','Hussar','80 food',11,30],
  ['capped-ram','Capped Ram','160 wood · 75 gold',19,31],
  ['siege-ram','Siege Ram','160 wood · 75 gold',19,32],
  ['onager','Onager','160 wood · 135 gold',19,33],
];
export const imperialLines=[
  ['castle-age','imperial-age'],['long-swordsman','two-handed-swordsman'],['two-handed-swordsman','champion'],
  ['pikeman','halberdier'],['crossbowman','arbalester'],['light-cavalry','hussar'],['cavalier','paladin'],
  ['capped-ram','siege-ram'],['iron-casting','blast-furnace'],['chain-mail-armor','plate-mail-armor'],
  ['chain-barding-armor','plate-barding-armor'],['bodkin-arrow','bracer'],['leather-archer-armor','ring-archer-armor'],
  ['heavy-plow','crop-rotation'],['bow-saw','two-man-saw'],['gold-shaft-mining','stone-shaft-mining'],
];
