import { actorLabels } from '/interface/dom/actors.js';
import { set, attribute } from '/interface/dom/update.js';
import { initBuildings } from '/interface/buildings.js';
import { initArt, icon as packIcon } from '/interface/ui-art.js';
import { actorKinds, buildingKinds, names } from '/interface/menu/catalog.js';
import { initSteward, initVassal } from '/interface/steward/panel.js';
import { initGeneralPanel } from '/interface/general-panel.js';
import { updateDetails } from '/interface/details.js';
import { initPause } from '/interface/pause.js';
import { initSaveGame } from '/interface/saves.js';
import { initRelics } from '/interface/relics/panel.js';
import { initActivity } from '/interface/activity/panel.js';
// Original interface artwork and DOM presentation. Authoritative values arrive
// from Rust at most four times per second; no frame loop or gameplay rules here.
const icon = name => name === 'relic' ? packIcon('units',285,'gold') : packIcon('resources',name,name);
const byId = id => document.getElementById(id);
const text = (id,value) => { const node = byId(id); if (node) set(node, 'textContent', String(value)); };

export function initInterface() {
  initBuildings();
  initActivity();
  initRelics();
  initSteward();
  initVassal();
  initGeneralPanel();
  initArt();
  const scene = () => byId('scene');
  const menu = byId('game-menu');
  byId('menu-toggle').addEventListener('click', () => {
    window.dispatchEvent(new Event('blur'));
    menu.showModal();
  });
  byId('resume-game').addEventListener('click', () => { menu.close(); scene().focus(); });
  byId('quit-game').addEventListener('click', () => { menu.close(); window.aoeStart.menu(); });
  byId('reconnect').addEventListener('click', () => menu.close());
  initPause(menu);
  initSaveGame();
  byId('reload-game').addEventListener('click', event => { event.preventDefault(); location.reload(); });
  let drag = null;
  const box = byId('drag-box');
  document.addEventListener('pointerdown', event => { if (event.target===scene() && event.button===0) drag=[event.clientX,event.clientY]; });
  document.addEventListener('pointermove', event => {
    if (!drag) return;
    const width=Math.abs(event.clientX-drag[0]), height=Math.abs(event.clientY-drag[1]);
    box.hidden = width<4 && height<4;
    Object.assign(box.style,{left:`${Math.min(drag[0],event.clientX)}px`,top:`${Math.min(drag[1],event.clientY)}px`,width:`${width}px`,height:`${height}px`});
  });
  for (const event of ['pointerup','pointercancel','lostpointercapture']) document.addEventListener(event,()=>{ drag=null;box.hidden=true; });
  let bottom = 208;
  const panel = byId('bottom-panel');
  new ResizeObserver(() => {
    bottom = panel.getBoundingClientRect().height || 208;
  }).observe(panel);
  const labels = actorLabels(icon);
  const update = state => {
    ['wood','food','gold','stone'].forEach((resource,index) => {
      text(`stockpile-${resource}`,state.stockpile[index]); text(`workers-${resource}`,state.workers[index]);
    });
    text('population',state.population.join('/'));
    text('idle-villager-count',state.idleVillagers);
    set(byId('idle-villagers'), 'disabled', state.idleVillagers===0);
    byId('idle-villagers').classList.toggle('has-idle',state.idleVillagers>0);
    text('idle-military-count',state.idleMilitary);
    set(byId('idle-military'), 'disabled', state.idleMilitary===0);
    byId('idle-military').classList.toggle('has-idle',state.idleMilitary>0);
    const minutes=Math.floor(state.seconds/60), seconds=state.seconds%60;
    text('game-clock',`${String(minutes).padStart(2,'0')}:${String(seconds).padStart(2,'0')}`);
    text('connection',state.status);
    byId('game-notice').classList.toggle('connected',state.status==='connected');
    window.aoeBuildings.selection(state);
    attribute(scene(),'data-actors',JSON.stringify(state.actors));
    attribute(scene(),'data-village-art',state.art?'native':'markers');
    labels.health(state.actors.filter(a=>a.selected && a.hp>0 && !buildingKinds.includes(a.kind)));
    const onMap = a => a.hp>0 && a.x>0 && a.y>54 && a.y<innerHeight-bottom;
    labels.idle(state.actors.filter(a=>onMap(a) && a.owned && a.task==='Idle' && (a.kind==='Villager' || a.kind==='FishingShip')));
    labels.flags(state.actors.filter(a=>onMap(a) && a.garrison>0));
    labels.loads(state.actors.filter(a=>onMap(a) && a.load));
  };
  window.aoeInterface = { details: updateDetails, numeric(values,status) {
    const tasks=['Idle','Moving','Gathering','Carrying','Returning to drop-off','Attacking','Dying','Building','Farming','Hunting','Shepherding','Fishing','Seeding','Idle','Fleeing','Attacking','Dying','Carcass','Packing','Unpacking'];
    const resources=['food','wood','gold','stone',''];
    const state={stockpile:values.slice(0,4),population:values.slice(4,6),workers:values.slice(6,10),seconds:values[10],art:values[11]===1,status,actors:[],selected:[],queue:null,trainingRows:[]};
    state.idleVillagers=values[13];
    state.idleMilitary=values[14];
    let at=15;
    for (let index=0;index<values[12];index++,at+=19) {
      const [id,kind,x,y,selected,task,resource,amount,hp,maxHp,attack,melee,pierce,head,capacity,range,player,owned,colour]=values.subarray(at,at+19);
      const actorKind = actorKinds[kind];
      const taskName = actorKind === 'FishingShip' && tasks[task] === 'Farming' ? 'Fishing' : tasks[task];
      const load = actorKind === 'Monk' && resource === 5
        ? ['relic', '']
        : actorKind === 'Villager' && amount ? [resources[resource], amount] : null;
      state.actors.push({id,player,owned:!!owned,kind:actorKind,x,y,selected:!!selected,hp,maxHp,head,task:taskName,load,garrison:owned&&buildingKinds.includes(actorKind)?capacity:0});
      if (selected) state.selected.push({id,player,owned:!!owned,colour,kind:actorKind,name:names[actorKind],task:taskName,resource:resources[resource],amount,hp,maxHp,attack,melee,pierce,capacity,range});
    }
    while (at < values.length) {
      const [id, kind, length, elapsed, total] = values.subarray(at, at + 5);
      at += 5;
      const units = Array.from(values.subarray(at, at + length));
      if (state.selected.length === 1) state.queue = { elapsed, total, units };
      else state.trainingRows.push([id, kind, units]);
      at += length;
    }
    update(state);
  }};
}
