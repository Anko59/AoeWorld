// Semantic metadata only. Prices, requirements and availability come from Rust.
// Order follows the protocol's UnitKind and Tech enums after Steward / StoneShaftMining.
export const gunpowderUnits=['CavalryArcher','HeavyCavalryArcher','CamelRider','HeavyCamelRider','Scorpion','HeavyScorpion','HandCannoneer','BombardCannon','Trebuchet','TrebuchetUnpacked','Petard','SiegeOnager'];
// DE DAT unit ids for icons and sprites.
export const gunpowderEntities={CavalryArcher:39,HeavyCavalryArcher:474,CamelRider:329,HeavyCamelRider:330,Scorpion:279,HeavyScorpion:542,HandCannoneer:5,BombardCannon:36,Trebuchet:331,TrebuchetUnpacked:42,Petard:440,SiegeOnager:588};
// DE DAT technology ids, in Tech order.
export const gunpowderTechIds=[47,218,236,239,320];
// [id, label, building kind index]
export const gunpowderTechs=[
  ['chemistry','Chemistry · 300 food · 200 gold',18],
  ['heavy-cavalry-archer','Heavy Cavalry Archer · 900 food · 500 gold',10],
  ['heavy-camel-rider','Heavy Camel Rider · 325 food · 360 gold',11],
  ['heavy-scorpion','Heavy Scorpion · 800 food · 750 wood',19],
  ['siege-onager','Siege Onager · 1450 food · 1000 gold',19],
];
// [id, label, cost, building kind index, UnitKind index]
export const gunpowderTraining=[
  ['cavalry-archer','Cavalry Archer','40 wood · 60 gold',10,48],
  ['heavy-cavalry-archer','Heavy Cavalry Archer','40 wood · 60 gold',10,49],
  ['hand-cannoneer','Hand Cannoneer','45 food · 50 gold · needs Chemistry',10,54],
  ['camel-rider','Camel Rider','55 food · 60 gold',11,50],
  ['heavy-camel-rider','Heavy Camel Rider','55 food · 60 gold',11,51],
  ['scorpion','Scorpion','75 wood · 75 gold',19,52],
  ['heavy-scorpion','Heavy Scorpion','75 wood · 75 gold',19,53],
  ['bombard-cannon','Bombard Cannon','225 wood · 225 gold · needs Chemistry',19,55],
  ['siege-onager','Siege Onager','160 wood · 135 gold',19,59],
  ['trebuchet','Trebuchet','200 wood · 200 gold · unpacks to fire',16,56],
  ['petard','Petard','65 food · 20 gold · explodes on buildings',16,58],
];
export const gunpowderLines=[['onager','siege-onager']];
export const gunpowderTrainLines=[
  ['train-cavalry-archer','train-heavy-cavalry-archer'],['train-camel-rider','train-heavy-camel-rider'],
  ['train-scorpion','train-heavy-scorpion'],['train-onager','train-siege-onager'],
];
