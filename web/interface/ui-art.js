import { castleUnits } from '/interface/castle.js';
import { imperialUnits } from '/interface/imperial.js';
import { genericTechArt } from '/interface/generic.js';
import { frame, FRAME_SPECS } from '/interface/panel-frame.js';
// The pack contains original pixels; this module contains only semantic IDs.
const entities = { Scout:448, Villager:83, TownCenter:109, House:70, LumberCamp:562,
  MiningCamp:584, Mill:68, Farm:50, Barracks:12, PalisadeWall:72, Outpost:598,
  Dock:45, ArcheryRange:87, Stable:101, Blacksmith:103, Market:84, WatchTower:79,
  PalisadeGate:792, StoneWall:117, StoneGate:64, Militia:74, ManAtArms:75, FishingShip:13, Sheep:594,
  Deer:65, Boar:48, Wolf:126, Archer:4, Skirmisher:7, Spearman:93, Castle:82, Monastery:104, University:209, SiegeWorkshop:49, LongSwordsman:77, Pikeman:358, Crossbowman:24, EliteSkirmisher:6, Knight:38, LightCavalry:546, Monk:125, Mangonel:280, BatteringRam:35, Galley:539, FireShip:529, DemolitionShip:527, TransportShip:545, WarGalley:21, Vassal:434, Steward:434, General:632,
  TwoHandedSwordsman:473, Champion:567, Halberdier:359, Arbalester:492, Cavalier:283, Paladin:569, Hussar:441, CappedRam:422, SiegeRam:548, Onager:550, TradeCart:128, TradeCog:17, Wonder:276 };
export const techIds = [22,8,101,202,14,55,278,213,222,67,74,81,199,211,102,207,197,100,98,254,249,280,68,76,82,200,212,13,203,182,50,93,321,34,103,217,264,429,237,209,265,428,96,255,257,75,77,80,201,219,12,221,279,...genericTechArt];
export const trainingKinds = ['Villager','Scout','Militia','FishingShip','Archer','Skirmisher','Spearman','ManAtArms',...castleUnits,'Galley','FireShip','DemolitionShip','TransportShip','WarGalley','Vassal','General',...imperialUnits,'Steward','TradeCart','TradeCog'];
let catalog = null;
const pageImages = new Map(), sprites = new Map();
let artDone; export const artReady = new Promise(resolve => { artDone = resolve; });
// Cached, down-scaled crops of the original icon art for canvas drawing.
export function iconSprite(group, id, size = 64) {
  const key = `${group}:${id}:${size}`;
  if (sprites.has(key)) return sprites.get(key);
  const art = catalog?.[group]?.[id], image = art && pageImages.get(art.page);
  if (!validArt(art) || !image) return null;
  const canvas = document.createElement('canvas'); canvas.width = canvas.height = size;
  const context = canvas.getContext('2d'); if (!context) return null;
  context.imageSmoothingQuality = 'high';
  context.drawImage(image, art.x, art.y, art.width, art.height, 0, 0, size, size);
  sprites.set(key, canvas); return canvas;
}
export const kindSprite = (kind, size = 64) => iconSprite('units', entities[kind], size);
// A data URL for a sub-rectangle of a panel; panels cover their container by
// nine-slicing this, never by stretching a fixed rectangle into a gap.
export function panelCrop(role, region) {
  const art = catalog?.panels?.[role], image = art && pageImages.get(art.page);
  if (!validArt(art) || !image) return '';
  const [x, y, width, height] = region ?? [0, 0, art.width, art.height];
  const canvas = document.createElement('canvas'); canvas.width = width; canvas.height = height;
  const context = canvas.getContext('2d'); if (!context) return '';
  context.drawImage(image, art.x + x, art.y + y, width, height, 0, 0, width, height);
  return canvas.toDataURL();
}
const fallback = name => `<svg class="icon fallback-icon" aria-hidden="true"><use href="/interface/icons.svg#${name}"></use></svg>`;
const validArt = art => art && /^(?:buildings\/)?[\w-]+\.(png|svg)$/.test(art.page)
  && ['x','y','width','height','page_width','page_height'].every(key => Number.isInteger(art[key]) && art[key]>=0)
  && art.width>0 && art.height>0 && art.page_width<=2048 && art.page_height<=2048
  && art.x+art.width<=art.page_width && art.y+art.height<=art.page_height;
const original = art => `<svg class="icon original-icon" aria-hidden="true" viewBox="0 0 ${art.width} ${art.height}"><image href="/asset-pack/${art.page}" x="${-art.x}" y="${-art.y}" width="${art.page_width}" height="${art.page_height}"/></svg>`;
// Imported unit portraits leave their player-colour regions transparent, so the
// owner's colour behind the icon (--tint) fills exactly those regions. The scene
// renderer (aoe_rendering::player_colour) resolves each unit's colour, including
// the chosen-colour swap, and sends it packed as 0xRRGGBB; this only formats it.
const tintPattern = /^(?:#[0-9a-f]{3,8}|rgb\(\d{1,3}, ?\d{1,3}, ?\d{1,3}\))$/i;
export function playerTint(player, owned, packed) {
  if (player < 0) return '';
  if (owned || player === undefined) {
    const own = document.documentElement.style.getPropertyValue('--player-colour').trim();
    if (tintPattern.test(own)) return own;
  }
  if (Number.isInteger(packed) && packed > 0) return `rgb(${packed>>16&255},${packed>>8&255},${packed&255})`;
  return '';
}
export function icon(group, id, symbol='villager', alt=-1, tint='') {
  const art = catalog?.[group]?.[id] ?? catalog?.[group]?.[alt];
  const style = tintPattern.test(tint) ? ` style="--tint:${tint}"` : '';
  return `<span class="art-icon" data-ui-group="${group}" data-ui-id="${id}" data-ui-alt="${alt}"${style}>${validArt(art)?original(art):fallback(symbol)}</span>`;
}
export function entityIcon(kind, tint) {
  if (kind==='Bridge') return icon('commands','build','towncenter');
  const symbol = ['Scout','Knight','LightCavalry','General','Cavalier','Paladin','Hussar'].includes(kind)?'scout':['Villager','Vassal','Steward'].includes(kind)?'villager':['FishingShip','Galley','FireShip','DemolitionShip','TransportShip','WarGalley'].includes(kind)?'ship':
    ['Militia','ManAtArms','Archer','Skirmisher','Spearman',...castleUnits,...imperialUnits].includes(kind)?'soldier':
    ['Sheep','Deer','Boar','Wolf'].includes(kind)?'animal':'towncenter';
  return icon('units', entities[kind] ?? -1, symbol, -1, trainingKinds.includes(kind) ? (tint ?? playerTint()) : '');
}
// Officers use imported hero portraits only; absent art is never substituted.
export function squadIcon(kind) {
  const id = [432,200,198,170][kind], art = catalog?.units?.[id];
  return `<span class="art-icon" data-ui-group="units" data-ui-id="${id}" data-native-only="true">${validArt(art)?original(art):''}</span>`;
}
export function setEntityIcon(node, kind, tint) {
  const key = `${kind}|${tint ?? ''}`;
  if (node.dataset.entityIcon===key) return;
  node.dataset.entityIcon=key;
  node.innerHTML=entityIcon(kind, tint);
}
function hydrate() {
  for (const node of document.querySelectorAll('.art-icon')) {
    const group = catalog?.[node.dataset.uiGroup];
    const art = group?.[node.dataset.uiId] ?? group?.[node.dataset.uiAlt];
    if (validArt(art)) node.innerHTML=original(art);
  }
}
// Percentage atlas coordinates keep every crop correct as its panel resizes.
function background(art) {
  return { image:`url('/asset-pack/${art.page}')`,
    size:`${100*art.page_width/art.width}% ${100*art.page_height/art.height}%`,
    position:`${100*art.x/(art.page_width-art.width || 1)}% ${100*art.y/(art.page_height-art.height || 1)}%` };
}
export function panelArt(node, role) {
  const style = document.documentElement.dataset.interfaceStyle ?? 'western';
  const art = catalog?.panels?.[`${style}-${role}`];
  if (!validArt(art)) return;
  const crop = background(art);
  node.style.backgroundImage = crop.image; node.style.backgroundSize = crop.size;
  node.style.backgroundPosition = crop.position;
  for (const iconNode of node.querySelectorAll('.art-icon:not([data-hydrated])')) {
    const cell = catalog?.[iconNode.dataset.uiGroup]?.[iconNode.dataset.uiId];
    if (validArt(cell)) { iconNode.innerHTML = original(cell); iconNode.dataset.hydrated = 'true'; }
  }
}
export function initArt() {
  document.documentElement.dataset.interfaceArt='fallback';
  for (const node of document.querySelectorAll('#top-bar .counter>.icon, #top-bar .worker-count>.icon, #train-villager .cost>.icon')) {
    const name=node.querySelector('use')?.getAttribute('href')?.split('#')[1];
    if (name) node.outerHTML=name==='villager' && !node.closest('.population')?entityIcon('Villager'):
      icon('resources',node.closest('.population')?'population':name,name);
  }
  for (const [selector,kind] of [['#idle-villagers>.icon','Villager'],['#idle-military>.icon','Militia'],['#town-center>.icon','TownCenter']]) {
    const node=document.querySelector(selector);if(node)node.outerHTML=entityIcon(kind);
  }
  Promise.all(['interface.json','buildings/interface.json'].map(path =>
    fetch(`/asset-pack/${path}`).then(response => response.ok?response.json():null).catch(() => null)
  )).then(async ([base, overlay]) => {
    if (!base && !overlay) return;
    const data=base ?? {};
    for (const group of ['units','technologies','commands','resources','panels']) if (overlay?.[group])
      data[group]={...data[group],...Object.fromEntries(
        Object.entries(overlay[group]).map(([id,art]) => [id,{...art,page:`buildings/${art.page}`}])
      )};
    const arts=Object.values(data).flatMap(group => Object.values(group));
    if (!arts.length || !arts.every(validArt)) return;
    await Promise.all([...new Set(arts.map(art=>art.page))].map(page=>new Promise((resolve,reject)=>{
      const image=new Image(); image.onload=()=>{pageImages.set(page,image);resolve();}; image.onerror=reject; image.src=`/asset-pack/${page}`;
    })));
    catalog=data;
    hydrate();
    artDone();
    const menu=catalog.panels?.['menu-background'];
    if (validArt(menu)) {
      const image=new Image();image.src=`/asset-pack/${menu.page}`;await image.decode();
      const canvas=document.createElement('canvas');canvas.width=menu.width;canvas.height=menu.height;
      const context=canvas.getContext('2d');
      if(context){
        context.drawImage(image,menu.x,menu.y,menu.width,menu.height,0,0,menu.width,menu.height);
        // Native menu crops exceed Chromium's CSS custom-property text limit.
        // The document owns this bounded blob URL for its lifetime.
        await new Promise(resolve=>canvas.toBlob(blob=>{
          if(blob) document.documentElement.style.setProperty('--ui-menu-image',`url('${URL.createObjectURL(blob)}')`);
          resolve(blob);
        }));
      }
    }
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
    const topArt=catalog.panels?.[`${style}-top`];
    if (validArt(topArt)) {
      const crop=background(topArt), node=document.getElementById('top-bar');
      node.style.backgroundImage=crop.image; node.style.backgroundSize=crop.size;
      node.style.backgroundPosition=crop.position;
    }
    // The three bottom panels are drawn at their native size; the frame
    // module cuts each to its opaque bounds so no art is smaller than its box.
    const frames=await Promise.all(Object.entries(FRAME_SPECS).map(([role,spec]) => {
      const art=catalog.panels?.[`${style}-${role}`];
      return validArt(art)?frame(document.querySelector(spec.selector),art,spec).catch(()=>null):null;
    }));
    const native=frames.every(Boolean);
    document.getElementById('bottom-panel').dataset.frame=native?'native':'none';
    // The native minimap frame is a 2:1 rhombus, the isometric projection itself.
    if (native && !document.getElementById('minimap').classList.contains('expanded')) document.getElementById('minimap-map').setAttribute('preserveAspectRatio','none');
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
    document.documentElement.dataset.interfaceArt=native&&validArt(topArt)?'original':'icons';
  }).catch(()=>{ /* A missing local pack retains the complete SVG interface. */ });
}
