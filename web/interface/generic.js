// Semantic metadata only. Prices, requirements and availability come from Rust.
// Generic Barracks, Archery Range, Stable, University, Monastery, Castle and
// Market technologies, in the protocol's Tech order after StoneShaftMining.
// [id, label, building kind index]
export const genericTechs=[
  ['squires','Squires · 100 food',6],
  ['arson','Arson · 75 food · 25 gold',6],
  ['gambesons','Gambesons · 50 food · 100 gold',6],
  ['thumb-ring','Thumb Ring · 300 food · 250 wood',10],
  ['parthian-tactics','Parthian Tactics · 200 food · 250 gold',10],
  ['bloodlines','Bloodlines · 150 food · 100 gold',11],
  ['husbandry','Husbandry · 150 food',11],
  ['architecture','Architecture · 300 food · 200 wood',18],
  ['siege-engineers','Siege Engineers · 500 food · 600 wood',18],
  ['heated-shot','Heated Shot · 350 food · 100 gold',18],
  ['arrowslits','Arrowslits · 250 food · 250 wood',18],
  ['fortified-wall','Fortified Wall · 200 food · 100 wood',18],
  ['guard-tower','Guard Tower · 100 food · 250 wood',18],
  ['keep','Keep · 500 food · 350 wood',18],
  ['devotion','Devotion · 100 food · 200 gold',17],
  ['faith','Faith · 550 food · 750 gold',17],
  ['fervor','Fervor · 140 gold',17],
  ['sanctity','Sanctity · 175 gold',17],
  ['herbal-medicine','Herbal Medicine · 200 gold',17],
  ['theocracy','Theocracy · 200 gold',17],
  ['illumination','Illumination · 120 gold',17],
  ['block-printing','Block Printing · 200 gold',17],
  ['atonement','Atonement · 325 gold',17],
  ['redemption','Redemption · 475 gold',17],
  ['heresy','Heresy · 1000 gold',17],
  ['conscription','Conscription · 150 food · 150 gold',16],
  ['hoardings','Hoardings · 400 food · 400 wood',16],
  ['sappers','Sappers · 400 food · 200 wood',16],
  ['spies','Spies/Treason · 200 gold',16],
  ['caravan','Caravan · 200 food · 200 gold',13],
  ['guilds','Guilds · 300 food · 200 gold',13],
  ['banking','Banking · 300 food · 200 gold',13],
  ['coinage','Coinage · 200 food · 100 gold',13],
  ['treadmill-crane','Treadmill Crane · 200 wood · 50 stone',18],
];
// DAT research ids, for the interface icon catalog.
export const genericTechArt=[215,602,875,437,436,435,39,51,377,380,608,194,140,63,46,45,252,231,441,438,233,230,319,316,439,315,379,321,408,48,15,17,23,54];
export const genericLines=[
  ['masonry','architecture'],['guard-tower','keep'],['coinage','banking'],['devotion','faith'],
];
