import { frame, FRAME_SPECS } from '/interface/panel-frame.js';
import { portraitIds, trainingKinds, techIds } from '/interface/menu/catalog.js';
export { trainingKinds, techIds };
let catalog = null;
const pageImages = new Map(), sprites = new Map();
let artDone; export const artReady = new Promise(resolve => { artDone = resolve; });
function cropCanvas(art, region, size) {
  if (!validArt(art)) return null;
  const [x, y, width, height] = region ?? [0, 0, art.width, art.height];
  const image = pageImages.get(art.page);
  if (!image) return null;
  const canvas = document.createElement('canvas');
  [canvas.width, canvas.height] = size ?? [width, height];
  const context = canvas.getContext('2d');
  if (!context) return null;
  context.imageSmoothingQuality = 'high';
  context.drawImage(image, art.x + x, art.y + y, width, height, 0, 0, canvas.width, canvas.height);
  return canvas;
}

export function kindSprite(kind, size = 64) {
  const key = `${kind}:${size}`;
  if (!sprites.has(key)) {
    const sprite = cropCanvas(catalog?.units?.[portraitIds[kind]], undefined, [size, size]);
    if (sprite) sprites.set(key, sprite);
  }
  return sprites.get(key) ?? null;
}

export const panelCrop = (role, region) => cropCanvas(catalog?.panels?.[role], region)?.toDataURL() ?? '';
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
  if (kind === 'Road') return icon('commands', 'repair', 'build');
  const symbol = kind === 'Villager' ? 'villager' : kind.includes('Ship') || kind.includes('Galley') ? 'ship'
    : trainingKinds.includes(kind) ? 'soldier' : ['Sheep', 'Deer', 'Boar', 'Wolf'].includes(kind) ? 'animal' : 'towncenter';
  return icon('units', portraitIds[kind] ?? -1, symbol, -1, trainingKinds.includes(kind) ? (tint ?? playerTint()) : '');
}
// Officers use imported hero portraits only; absent art is never substituted.
export function squadIcon(kind) {
  const id = [432,200,198,170][kind], art = catalog?.units?.[id];
  return `<span class="art-icon" data-ui-group="units" data-ui-id="${id}" data-native-only="true">${validArt(art)?original(art):''}</span>`;
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
function paintBackground(id, art) {
  if (!validArt(art)) return false;
  const { image, size, position } = background(art);
  Object.assign(document.getElementById(id).style, {
    backgroundImage: image, backgroundSize: size, backgroundPosition: position,
  });
  return true;
}
export function initArt() {
  document.documentElement.dataset.interfaceArt='fallback';
  for (const node of document.querySelectorAll('#top-bar .counter>.icon, #top-bar .worker-count>.icon')) {
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
    const sliders = ['track', 'fill', 'thumb', 'thumb-hover', 'thumb-active'];
    const crops = sliders.map(role => panelCrop('slider-' + role));
    if (crops.every(Boolean)) {
      sliders.forEach((role, index) => document.documentElement.style.setProperty('--slider-' + role, `url('${crops[index]}')`));
      document.getElementById('bottom-panel').dataset.sliderArt = 'native';
    }
    artDone();
    const menu = cropCanvas(catalog.panels?.['menu-background']);
    if (menu) {
      // The full crop exceeds Chromium's CSS custom-property text limit.
      const blob = await new Promise(resolve => menu.toBlob(resolve));
      if (blob) document.documentElement.style.setProperty('--ui-menu-image', `url('${URL.createObjectURL(blob)}')`);
    }
    for (const [role, key] of [['attack','attack'],['build','build-cursor'],['garrison','garrison']]) {
      const art = catalog.commands?.[key];
      if (art?.width > 64 || art?.height > 64) continue;
      const cursor = cropCanvas(art);
      if (cursor) document.documentElement.style.setProperty(`--cursor-${role}`, `url('${cursor.toDataURL()}') 0 0`);
    }
    const style = document.documentElement.dataset.interfaceStyle ?? 'western';
    const hasTop = paintBackground('top-bar', catalog.panels?.[`${style}-top`]);
    // The three bottom panels are drawn at their native size; the frame
    // module cuts each to its opaque bounds so no art is smaller than its box.
    const frames=await Promise.all(Object.entries(FRAME_SPECS).map(([role,spec]) => {
      const art=catalog.panels?.[`${style}-${role}`];
      return validArt(art)?frame(document.querySelector(spec.selector),art,pageImages.get(art.page),spec).catch(()=>null):null;
    }));
    const native=frames.every(Boolean);
    document.getElementById('bottom-panel').dataset.frame=native?'native':'none';
    // The native minimap frame is a 2:1 rhombus, the isometric projection itself.
    if (native && !document.getElementById('minimap').classList.contains('expanded')) document.getElementById('minimap-map').setAttribute('preserveAspectRatio','none');
    paintBackground('selection-portrait', catalog.panels?.portrait);
    const button=catalog.panels?.button;
    if (validArt(button)) {
      const crop=background(button), style=document.documentElement.style;
      style.setProperty('--ui-button-image',crop.image); style.setProperty('--ui-button-size',crop.size);
      style.setProperty('--ui-button-position',crop.position);
    }
    document.documentElement.dataset.interfaceArt=native && hasTop ? 'original' : 'icons';
  }).catch(()=>{ /* A missing local pack retains the complete SVG interface. */ });
}
