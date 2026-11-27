// The metadata follows protocol order. Rules and eligibility arrive from the game.
const words = text => text.trim().split(/\s+/);
export const trainingKinds = words(`
  Villager Scout Militia FishingShip Archer Skirmisher Spearman ManAtArms
  LongSwordsman Pikeman Crossbowman EliteSkirmisher Knight LightCavalry Monk Mangonel
  BatteringRam Galley FireShip DemolitionShip TransportShip WarGalley Vassal General
  TwoHandedSwordsman Champion Halberdier Arbalester Cavalier Paladin Hussar CappedRam
  SiegeRam Onager Steward TradeCart TradeCog FireGalley FastFireShip DemolitionRaft
  HeavyDemolitionShip Galleon CannonGalleon EliteCannonGalleon Hulk WarHulk Carrack
  CatapultGalleon CavalryArcher HeavyCavalryArcher CamelRider HeavyCamelRider Scorpion
  HeavyScorpion HandCannoneer BombardCannon Trebuchet TrebuchetUnpacked Petard
  SiegeOnager
`);
export const buildingKinds = words(`
  TownCenter House LumberCamp MiningCamp Mill Farm Barracks PalisadeWall Outpost
  Dock ArcheryRange Stable Blacksmith Market WatchTower PalisadeGate Castle
  Monastery University SiegeWorkshop Bridge StoneWall StoneGate Road Wonder FishTrap
  GuardTower Keep
`);
// Upgraded towers have selection cards but no foundation commands.
export const buildableKinds = buildingKinds.filter(kind => kind !== 'GuardTower' && kind !== 'Keep');
export const techKinds = words(`
  Loom TownWatch FeudalAge DoubleBitAxe HorseCollar GoldMining StoneMining
  Wheelbarrow ManAtArms Forging ScaleMailArmor ScaleBardingArmor Fletching
  PaddedArcherArmor CastleAge LongSwordsman Pikeman Crossbowman EliteSkirmisher
  LightCavalry HandCart TownPatrol IronCasting ChainMailArmor ChainBardingArmor
  BodkinArrow LeatherArcherArmor HeavyPlow BowSaw GoldShaftMining Masonry Ballistics
  MurderHoles WarGalley ImperialAge TwoHandedSwordsman Champion Halberdier Arbalester
  Cavalier Paladin Hussar CappedRam SiegeRam Onager BlastFurnace PlateMailArmor
  PlateBardingArmor Bracer RingArcherArmor CropRotation TwoManSaw StoneShaftMining
  Squires Arson Gambesons ThumbRing ParthianTactics Bloodlines Husbandry Architecture
  SiegeEngineers HeatedShot Arrowslits FortifiedWall GuardTower Keep Devotion Faith
  Fervor Sanctity HerbalMedicine Theocracy Illumination BlockPrinting Atonement
  Redemption Heresy Conscription Hoardings Sappers Spies Caravan Guilds Banking
  Coinage TreadmillCrane FishingLines Gillnets Careening DryDock Shipwright
  ClinkerConstruction CarvelHull Siphons Incendiaries HeavyWarships DemolitionShip
  HeavyDemolitionShip EliteCannonGalleon Chemistry HeavyCavalryArcher HeavyCamelRider
  HeavyScorpion SiegeOnager
`);

export const portraitIds = { Scout:448, Villager:83, TownCenter:109, House:70, LumberCamp:562,
  MiningCamp:584, Mill:68, Farm:50, Barracks:12, PalisadeWall:72, Outpost:598,
  Dock:45, ArcheryRange:87, Stable:101, Blacksmith:103, Market:84, WatchTower:79, GuardTower:234, Keep:235, FishTrap:199,
  PalisadeGate:792, StoneWall:117, StoneGate:487, Militia:74, ManAtArms:75, FishingShip:13, Sheep:594,
  Deer:65, Boar:48, Wolf:126, Archer:4, Skirmisher:7, Spearman:93, Castle:82, Monastery:104, University:209, SiegeWorkshop:49, LongSwordsman:77, Pikeman:358, Crossbowman:24, EliteSkirmisher:6, Knight:38, LightCavalry:546, Monk:125, Mangonel:280, BatteringRam:35, Galley:539, FireShip:529, DemolitionShip:527, TransportShip:545, WarGalley:21, Vassal:434, Steward:434, General:632,
  TwoHandedSwordsman:473, Champion:567, Halberdier:359, Arbalester:492, Cavalier:283, Paladin:569, Hussar:441, CappedRam:422, SiegeRam:548, Onager:550, TradeCart:128, TradeCog:17, Wonder:276,  FireGalley:1103,FastFireShip:532,DemolitionRaft:1104,HeavyDemolitionShip:528,Galleon:442,CannonGalleon:420,EliteCannonGalleon:691,Hulk:2626,WarHulk:2627,Carrack:2628,CatapultGalleon:2633, CavalryArcher:39,HeavyCavalryArcher:474,CamelRider:329,HeavyCamelRider:330,Scorpion:279,HeavyScorpion:542,HandCannoneer:5,BombardCannon:36,Trebuchet:331,TrebuchetUnpacked:42,Petard:440,SiegeOnager:588, };
export const techIds = [22,8,101,202,14,55,278,213,222,67,74,81,199,211,102,207,197,100,98,254,249,280,68,76,82,200,212,13,203,182,50,93,322,34,103,217,264,429,237,209,265,428,96,255,257,75,77,80,201,219,12,221,279,215,602,875,437,436,435,39,51,377,380,608,194,140,63,46,45,252,231,441,438,233,230,319,316,439,315,379,321,408,48,15,17,23,54,906,65,374,375,373,908,907,909,910,35,905,244,376,47,218,236,239,320];

const readable = kind => kind.replace(/([a-z])([A-Z])/g, '$1 $2');
export const id = kind => readable(kind).toLowerCase().replaceAll(' ', '-');
export const names = {
  ...Object.fromEntries([...trainingKinds, ...buildingKinds, ...techKinds].map(kind => [kind, readable(kind)])),
  Scout: 'Scout Cavalry', ManAtArms: 'Man-at-Arms', TwoHandedSwordsman: 'Two-Handed Swordsman',
  TrebuchetUnpacked: 'Trebuchet (unpacked)', Spies: 'Spies/Treason', DoubleBitAxe: 'Double-Bit Axe',
  Sheep: 'Sheep', Deer: 'Deer', Boar: 'Boar', Wolf: 'Wolf',
};
export const buildingNames = buildingKinds.map(kind => names[kind]);
export const techName = kind => kind === 'WarGalley' ? 'Medium Warships' : names[kind];

// The native HUD sends protocol unit kinds, then buildings and wildlife.
export const actorKinds = [...trainingKinds, ...buildingKinds, 'Sheep', 'Deer', 'Boar', 'Wolf', 'Relic'];

export const price = amounts => amounts.map((amount, index) =>
  amount ? amount + ' ' + ['food', 'wood', 'gold', 'stone'][index] : '').filter(Boolean).join(' · ');
