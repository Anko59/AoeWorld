import { castleUnits } from '/interface/castle.js';
// The pack contains original pixels; this module contains only semantic IDs.
const entities = { Scout:448, Villager:83, TownCenter:109, House:70, LumberCamp:562,
  MiningCamp:584, Mill:68, Farm:50, Barracks:12, PalisadeWall:72, Outpost:598,
  Dock:45, ArcheryRange:87, Stable:101, Blacksmith:103, Market:84, WatchTower:79,
  PalisadeGate:792, Militia:74, ManAtArms:75, FishingShip:13, Sheep:594,
  Deer:65, Boar:48, Wolf:126, Archer:4, Skirmisher:7, Spearman:93, Castle:82, Monastery:104, University:209, SiegeWorkshop:49, LongSwordsman:77, Pikeman:358, Crossbowman:24, EliteSkirmisher:6, Knight:38, LightCavalry:546, Monk:125, Mangonel:280, BatteringRam:35, Galley:539, FireShip:529, DemolitionShip:527, TransportShip:545, WarGalley:21, Vassal:434 };
export const techIds = [22,8,101,202,14,55,278,213,222,67,74,81,199,211,102,207,197,100,98,254,249,280,68,76,82,200,212,13,203,182,50,93,321,34];
export const trainingKinds = ['Villager','Scout','Militia','FishingShip','Archer','Skirmisher','Spearman','ManAtArms',...castleUnits,'Galley','FireShip','DemolitionShip','TransportShip','WarGalley','Vassal'];
let catalog = null;
const fallback = name => `<svg class="icon fallback-icon" aria-hidden="true"><use href="/interface/icons.svg#${name}"></use></svg>`;
const validArt = art => art && /^[\w-]+\.(png|svg)$/.test(art.page)
  && ['x','y','width','height','page_width','page_height'].every(key => Number.isInteger(art[key]) && art[key]>=0)
  && art.width>0 && art.height>0 && art.page_width<=2048 && art.page_height<=2048
  && art.x+art.width<=art.page_width && art.y+art.height<=art.page_height;
const original = art => `<svg class="icon original-icon" aria-hidden="true" viewBox="0 0 ${art.width} ${art.height}"><image href="/asset-pack/${art.page}" x="${-art.x}" y="${-art.y}" width="${art.page_width}" height="${art.page_height}"/></svg>`;
export function icon(group, id, symbol='villager') {
  const art = catalog?.[group]?.[id];
  return `<span class="art-icon" data-ui-group="${group}" data-ui-id="${id}">${validArt(art)?original(art):fallback(symbol)}</span>`;
}
export function entityIcon(kind) {
  const symbol = ['Scout','Knight','LightCavalry'].includes(kind)?'scout':['Villager','Vassal'].includes(kind)?'villager':['FishingShip','Galley','FireShip','DemolitionShip','TransportShip','WarGalley'].includes(kind)?'ship':
    ['Militia','ManAtArms','Archer','Skirmisher','Spearman',...castleUnits].includes(kind)?'soldier':
    ['Sheep','Deer','Boar','Wolf'].includes(kind)?'animal':'towncenter';
  return icon('units', entities[kind] ?? -1, symbol);
}
export function setEntityIcon(node, kind) {
  if (node.dataset.entityIcon===kind) return;
  node.dataset.entityIcon=kind;
  node.innerHTML=entityIcon(kind);
}
function hydrate() {
  for (const node of document.querySelectorAll('.art-icon')) {
    const art = catalog?.[node.dataset.uiGroup]?.[node.dataset.uiId];
    if (validArt(art)) node.innerHTML=original(art);
  }
}
// Percentage atlas coordinates keep every crop correct as its panel resizes.
function background(art) {
  return { image:`url('/asset-pack/${art.page}')`,
    size:`${100*art.page_width/art.width}% ${100*art.page_height/art.height}%`,
    position:`${100*art.x/(art.page_width-art.width || 1)}% ${100*art.y/(art.page_height-art.height || 1)}%` };
}
export function initArt() {
  document.documentElement.dataset.interfaceArt='fallback';
  for (const node of document.querySelectorAll('#top-bar .counter>.icon, #top-bar .worker-count>.icon')) {
    const name=node.querySelector('use')?.getAttribute('href')?.split('#')[1];
    if (name) node.outerHTML=name==='villager' && !node.closest('.population')?entityIcon('Villager'):
      icon('resources',node.closest('.population')?'population':name,name);
  }
  fetch('/asset-pack/interface.json').then(response => response.ok?response.json():null).then(async data => {
    if (!data) return;
    const arts=Object.values(data).flatMap(group => Object.values(group));
    if (!arts.length || !arts.every(validArt)) return;
    await Promise.all([...new Set(arts.map(art=>art.page))].map(page=>new Promise((resolve,reject)=>{
      const image=new Image(); image.onload=resolve; image.onerror=reject; image.src=`/asset-pack/${page}`;
    })));
    catalog=data;
    hydrate();
    // Crop original cursor pixels only in browser memory; no derived assets
    // are published. Firefox accepts these bounded PNG cursor URLs.
    for (const [role,key] of [['attack','attack'],['build','build-cursor'],['garrison','garrison']]) {
      const art=catalog.commands?.[key];
      if (!validArt(art) || art.width>64 || art.height>64) continue;
      const image=new Image(); image.src=`/asset-pack/${art.page}`; await image.decode();
      const canvas=document.createElement('canvas');canvas.width=art.width;canvas.height=art.height;
      const context=canvas.getContext('2d');if(!context)continue;
      context.drawImage(image,art.x,art.y,art.width,art.height,0,0,art.width,art.height);
      document.documentElement.style.setProperty(`--cursor-${role}`,`url('${canvas.toDataURL()}') 0 0`);
    }
    const style=document.documentElement.dataset.interfaceStyle ?? 'western';
    const roles=[['commands','commands'],['selection','selection'],['minimap','minimap'],['top-bar','top']];
    for (const [id,role] of roles) {
      const art=catalog.panels?.[`${style}-${role}`];
      if (!validArt(art)) continue;
      const crop=background(art), node=document.getElementById(id);
      node.style.backgroundImage=crop.image; node.style.backgroundSize=crop.size;
      node.style.backgroundPosition=crop.position;
    }
    const portrait=catalog.panels?.portrait;
    if (validArt(portrait)) {
      const crop=background(portrait), node=document.getElementById('selection-portrait');
      node.style.backgroundImage=crop.image; node.style.backgroundSize=crop.size; node.style.backgroundPosition=crop.position;
    }
    const button=catalog.panels?.button;
    if (validArt(button)) {
      const crop=background(button), style=document.documentElement.style;
      style.setProperty('--ui-button-image',crop.image); style.setProperty('--ui-button-size',crop.size);
      style.setProperty('--ui-button-position',crop.position);
    }
    document.documentElement.dataset.interfaceArt=roles.every(([,role])=>validArt(catalog.panels?.[`${style}-${role}`]))?'original':'icons';
  }).catch(()=>{ /* A missing local pack retains the complete SVG interface. */ });
}
