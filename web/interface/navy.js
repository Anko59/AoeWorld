// Semantic metadata only. Prices, requirements and availability come from Rust.
// Naval Warfare Overhaul (DE update 169123); order follows the protocol's
// UnitKind and Tech enums after Steward / StoneShaftMining.
export const navyUnits=['FireGalley','FastFireShip','DemolitionRaft','HeavyDemolitionShip','Galleon','CannonGalleon','EliteCannonGalleon','Hulk','WarHulk','Carrack','CatapultGalleon'];
// DAT unit ids of the icons.
export const navyEntities={FireGalley:1103,FastFireShip:532,DemolitionRaft:1104,HeavyDemolitionShip:528,Galleon:442,CannonGalleon:420,EliteCannonGalleon:691,Hulk:2626,WarHulk:2627,Carrack:2628,CatapultGalleon:2633};
// [id, label, building kind index (Dock 9, University 18), DAT technology id]
export const navyTechs=[
  ['fishing-lines','Fishing Lines · 50 food · 100 wood',9,906],
  ['gillnets','Gillnets · 100 food · 125 wood',9,65],
  ['careening','Careening · 100 food · 200 gold',18,374],
  ['dry-dock','Dry Dock · 200 food · 400 gold',18,375],
  ['shipwright','Shipwright · 1000 food · 300 gold',18,373],
  ['clinker-construction','Clinker Construction · 150 food · 100 wood',18,908],
  ['carvel-hull','Carvel Hull · 350 food · 200 wood',18,907],
  ['siphons','Siphons · 100 food · 175 gold',18,909],
  ['incendiaries','Incendiaries · 200 food · 325 gold',18,910],
  ['heavy-warships','Heavy Warships · 400 wood · 315 gold',9,35],
  ['demolition-ship','Demolition Ship · 25 wood · 75 gold',9,905],
  ['heavy-demolition-ship','Heavy Demolition Ship · 250 wood · 300 gold',9,244],
  ['elite-cannon-galleon','Elite Cannon Galleon · 525 wood · 500 gold',9,376],
];
// A line keeps its grid cell when its earlier step disappears.
export const navyLines=[['fishing-lines','gillnets'],['careening','dry-dock'],['clinker-construction','carvel-hull'],['siphons','incendiaries'],['war-galley','heavy-warships'],['demolition-ship','heavy-demolition-ship']];
export const navyTrainLines=[['galley','war-galley'],['war-galley','galleon'],['fire-galley','fire-ship'],['fire-ship','fast-fire-ship'],['hulk','war-hulk'],['war-hulk','carrack'],['demolition-raft','demolition-ship'],['demolition-ship','heavy-demolition-ship'],['cannon-galleon','elite-cannon-galleon']];
// [id, name, UnitKind name, cost]; one cell per line, in Dock order.
export const navyTraining=[
  ['galley','Galley','Galley','90 wood · 30 gold'],
  ['fire-galley','Fire Galley','FireGalley','75 wood · 45 gold'],
  ['hulk','Hulk','Hulk','75 wood · 35 gold'],
  ['demolition-raft','Demolition Raft','DemolitionRaft','45 wood · 80 gold'],
  ['transport-ship','Transport Ship','TransportShip','125 wood'],
  ['trade-cog','Trade Cog','TradeCog','100 wood · 50 gold'],
  ['cannon-galleon','Cannon Galleon','CannonGalleon','200 wood · 150 gold'],
  ['catapult-galleon','Catapult Galleon','CatapultGalleon','225 wood · 100 gold'],
  ['war-galley','War Galley','WarGalley','90 wood · 30 gold'],
  ['galleon','Galleon','Galleon','90 wood · 30 gold'],
  ['fire-ship','Fire Ship','FireShip','75 wood · 45 gold'],
  ['fast-fire-ship','Fast Fire Ship','FastFireShip','75 wood · 45 gold'],
  ['war-hulk','War Hulk','WarHulk','75 wood · 35 gold'],
  ['carrack','Carrack','Carrack','75 wood · 35 gold'],
  ['demolition-ship','Demolition Ship','DemolitionShip','45 wood · 80 gold'],
  ['heavy-demolition-ship','Heavy Demolition Ship','HeavyDemolitionShip','45 wood · 80 gold'],
  ['elite-cannon-galleon','Elite Cannon Galleon','EliteCannonGalleon','200 wood · 150 gold'],
];
// Grid cells the Dock's unit lines take after the Fishing Ship; research follows them.
export const dockUnitCells=9;
