(async () => {
const $ = s => document.querySelector(s);
// In-page helpers for selection-fit.play.spec.ts: they put the selection panel into each
// state that can overflow and measure it. Real HUD writes are muted so the state stays.
const ui = await import('/interface/ui-art.js');
const grid = await import('/interface/selection-grid.js');
const H = window.__h = {};
const O = {};
for (const [obj, fns] of [['aoeInterface',['numeric','details']],['aoeGeneral',['update']],['aoeGarrison',['update']],['aoeSteward',['update']],['aoeVassal',['update']],['aoeBuildings',['update','queue','refresh','preview']]])
  for (const fn of fns) { O[obj+'.'+fn] = window[obj][fn].bind(window[obj]); window[obj][fn] = () => {}; }
const frames = () => new Promise(r => setTimeout(r, 120));
const tint = ui.playerTint(0, true);
const clearAll = () => {
  O['aoeGeneral.update']([]); O['aoeGarrison.update']([0, 0]); O['aoeSteward.update']([], '', []);
  O['aoeVassal.update']([], '');
  for (const id of ['building-panel','training','garrison-panel','steward-status','steward-controls','vassal-panel','general-panel']) $('#'+id).hidden = true;
  $('#selected-icons').replaceChildren(); $('#training-queue').replaceChildren();
  for (const id of ['cancel-construction','cancel-research','farm-reseed-queue','construction-progress','research-progress']) $('#'+id).hidden = true;
  $('#research-icon').replaceChildren(); $('#research-label').textContent=''; $('#market-prices').textContent='';
  $('#unit-hp').hidden = true; $('#unit-combat').hidden = true; $('#selection-health').hidden = true; $('#selection-load').textContent='';
};
const setSel = (count, kind, name, o = {}) => {
  clearAll();
  const s = $('#selection'); s.dataset.count = String(count); s.dataset.kind = kind; s.dataset.owned = 'true';
  $('#selection-empty').hidden = true; $('#selection-content').hidden = false;
  $('#selection-name').textContent = name; $('#selection-task').textContent = o.task ?? '';
  $('#selection-load').textContent = o.load ?? '';
  $('#selection-owner').textContent = 'Your village';
  if (count === 1) ui.setEntityIcon($('#selection-portrait'), kind, tint);
  if (o.unit) { $('#unit-hp').hidden = false; $('#unit-combat').hidden = false; $('#selection-health').hidden = false; $('#unit-hp').textContent='40 / 40 HP'; $('#unit-combat').textContent='Attack 4 · Armour 0 / 0 · Range 0'; }
};
const mixes = { villagers:['Villager'], mixed:['Villager','Militia','Archer','Spearman','Skirmisher','Scout','ManAtArms'], many:['Villager','Militia','Archer','Spearman','Skirmisher','Scout','ManAtArms','Knight','Galley','FishingShip','Sheep','Deer'] };
H.multi = async (n, mix='mixed') => { const kinds = mixes[mix];
  setSel(n, kinds[0], `${n} units selected`, {task:'Right-click to give this group an order'});
  const units=Array.from({length:n},(_,i)=>({id:i+1,kind:kinds[i%kinds.length],name:kinds[i%kinds.length],tint}));
  grid.renderSelectionIcons($('#selected-icons'), units, ui.entityIcon);
  await frames(); };
H.unit = async () => { setSel(1,'Villager','Villager',{task:'Gathering food', load:'Carrying 12/20 food', unit:true}); await frames(); };
const squads = n => { const a=[]; for (let i=0;i<n;i++) a.push(100+i, i%4, [20,15,10,5][i%4]-i%3, i%4); return a; };
H.general = async (mode='captain', n=10) => { setSel(1,'General','General',{task:'Marching', load:'', unit:true});
  O['aoeGeneral.update']([11,200,1,0,0,20,30,30,10,10,60,60,40,20,20,n,...squads(n)]);
  $('#general-'+mode).click(); await frames(); };
H.steward = async () => { setSel(1,'Steward','Steward',{task:'Managing', unit:true});
  O['aoeSteward.update']([], 'Steward: 23 villagers · food 40% wood 30% gold 20% stone 10% · cap 50 · training on', new Uint8Array([1,40,30,20,10,50,1])); await frames(); };
H.tc = async (queue=15, garrison=15) => { setSel(1,'TownCenter','Town Center',{task:'Training', load:'Resource drop-off · supports 5 population'});
  $('#building-panel').hidden=false; $('#building-hp').textContent='2400 / 2400 HP'; $('#building-status').textContent='Complete';
  $('#research-label').textContent='Wheelbarrow · 40%'; $('#research-progress').hidden=false; $('#research-progress').value=40; $('#cancel-research').hidden=false;
  $('#research-icon').innerHTML = ui.icon('technologies', 1, 'technology');
  $('#training').hidden=false; $('#training-label').textContent='Training Villager · 40%';
  $('#training-queue').innerHTML = Array.from({length:queue},()=>`<span class="unit-token" title="queued">${ui.entityIcon('Villager')}</span>`).join('');
  const g=[]; for(let i=0;i<garrison;i++) g.push(i+1,0,25,25); O['aoeGarrison.update']([15,1,...g]); await frames(); };
H.castle = async () => { setSel(1,'Castle','Castle',{task:'Training', load:'Supports 20 population'});
  $('#building-panel').hidden=false; $('#building-hp').textContent='4800 / 4800 HP'; $('#building-status').textContent='Complete';
  $('#training').hidden=false; $('#training-queue').innerHTML = Array.from({length:15},()=>`<span class="unit-token">${ui.entityIcon('Archer')}</span>`).join('');
  const g=[]; for(let i=0;i<20;i++) g.push(i+1,0,25,25); O['aoeGarrison.update']([20,1,...g]);
  O['aoeVassal.update']([1,1,1],'Vassal: defending the domain · 3 threats nearby · garrison 20/20'); await frames(); };
H.market = async () => { setSel(1,'Market','Market',{task:'Building', load:''});
  $('#building-panel').hidden=false; $('#building-hp').textContent='1800 / 1800 HP'; $('#building-status').textContent='Complete';
  $('#market-prices').textContent='Prices per 100: wood 130 · food 90 · stone 160 gold'; await frames(); };
H.foundation = async () => { setSel(1,'Farm','Farm',{task:'Building'});
  $('#building-panel').hidden=false; $('#building-hp').textContent='100 / 100 HP'; $('#building-status').textContent='Constructing · 37%';
  $('#construction-progress').hidden=false; $('#cancel-construction').hidden=false; $('#farm-reseed-queue').hidden=false; await frames(); };
// Overflow measurement.
const vis = el => { const r = el.getBoundingClientRect(); const cs = getComputedStyle(el); return r.width > 1.5 && r.height > 1.5 && cs.visibility !== 'hidden' && cs.display !== 'none' && !el.closest('[hidden]'); };
H.measure = () => {
  const out = { scroll: [], clip: [] };
  const root = $('#selection');
  for (const sel of ['#selection','#selection-content','#selected-icons','#garrison-units','#general-squads','#general-panel','#steward-controls','#vassal-panel','#training','#training-queue','#building-panel']) {
    const el = $(sel); if (!el || !vis(el)) continue;
    if (el.scrollHeight > el.clientHeight + 2 || el.scrollWidth > el.clientWidth + 2) out.scroll.push(`${sel} ${el.scrollWidth}x${el.scrollHeight} > ${el.clientWidth}x${el.clientHeight}`);
  }
  const clipBox = (() => { const r = $('#selection-content').getBoundingClientRect(); return r; })();
  for (const el of $('#selection-content').querySelectorAll('*')) {
    if (!vis(el)) continue;
    if (el.closest('svg') && el.tagName !== 'svg') continue;
    const r = el.getBoundingClientRect();
    for (let a = el.parentElement; a && a !== root; a = a.parentElement) {
      const cs = getComputedStyle(a); if (cs.overflow === 'visible') continue;
      const b = a.getBoundingClientRect();
      if (r.bottom > b.bottom + 0.5 || r.right > b.right + 0.5 || r.top < b.top - 0.5 || r.left < b.left - 0.5) { out.clip.push(`${el.id ? '#'+el.id : el.tagName+'.'+el.className} clipped by #${a.id||a.className} (${Math.round(r.bottom-b.bottom)}px down, ${Math.round(r.right-b.right)}px right)`); break; }
    }
  }
  // Text must stay inside the parchment (the content box), and never be cut off by ellipsis or overflow.
  const box = $('#selection-content').getBoundingClientRect();
  const walker = document.createTreeWalker($('#selection-content'), NodeFilter.SHOW_TEXT);
  for (let n; (n = walker.nextNode());) {
    if (!n.textContent.trim() || !n.parentElement || !vis(n.parentElement)) continue;
    const range = document.createRange(); range.selectNodeContents(n);
    for (const r of range.getClientRects()) if (r.width > 0 && (r.right > box.right + 0.5 || r.left < box.left - 0.5 || r.bottom > box.bottom + 3 || r.top < box.top - 3)) { out.clip.push(`text "${n.textContent.trim().slice(0, 30)}" leaves the parchment (${Math.round(r.right - box.right)}px right, ${Math.round(r.bottom - box.bottom)}px down)`); break; }
  }
  for (const el of $('#selection-content').querySelectorAll('*')) {
    if (!vis(el) || getComputedStyle(el).overflow === 'visible' || el.id === 'selected-icons') continue;
    if (el.scrollWidth > el.clientWidth + 2 && el.children.length === 0) out.clip.push(`#${el.id || el.className} text truncated (${el.scrollWidth} > ${el.clientWidth})`);
  }
  out.clip = [...new Set(out.clip)].slice(0, 8);
  const ic = $('#selected-icons'); out.icons = ic && vis(ic) ? { layout: ic.dataset.layout, token: ic.style.getPropertyValue('--token'), tokens: ic.children.length, w: ic.clientWidth, h: ic.clientHeight, minTok: Math.min(...[...ic.children].map(c=>c.getBoundingClientRect().width)), rows: new Set([...ic.children].map(c=>Math.round(c.getBoundingClientRect().top))).size, first: (r => `${Math.round(r.width)}x${Math.round(r.height)}`)(ic.children[0]?.getBoundingClientRect() ?? {width:0,height:0}), html: ic.children[0]?.outerHTML.slice(0, 260) } : null;
  out.viewport = `${innerWidth}x${innerHeight}`; out.content = (() => { const r=$('#selection-content').getBoundingClientRect(); return `${Math.round(r.width)}x${Math.round(r.height)}`; })();
  return out;
};
return 'ready';
})();
