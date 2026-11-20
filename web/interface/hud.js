import { actorLabels } from '/interface/dom/actors.js';
import { set, attribute } from '/interface/dom/update.js';
import { castleKinds } from '/interface/castle.js';
import { initBuildings, buildingKinds, buildingNames } from '/interface/buildings.js';
import { initArt, icon as packIcon, entityIcon, setEntityIcon, playerTint } from '/interface/ui-art.js';
import { initVassal } from '/interface/steward/vassal.js';
import { initSteward } from '/interface/steward/panel.js';
import { initGeneralPanel } from '/interface/general-panel.js';
import { updateDetails } from '/interface/details.js';
import { renderSelectionIcons, autoFit } from '/interface/selection-grid.js';
import { initPause } from '/interface/pause.js';
import { initSaveGame } from '/interface/saves.js';
import { initActivity } from '/interface/activity/panel.js';
// Original interface artwork and DOM presentation. Authoritative values arrive
// from Rust at most four times per second; no frame loop or gameplay rules here.
const icon = name => packIcon('resources',name,name);
const byId = id => document.getElementById(id);
const text = (id,value) => { const node = byId(id); if (node) set(node, 'textContent', String(value)); };
const animals = ['Sheep','Deer','Boar','Wolf'];
const specialNames = {Scout:'Scout Cavalry', ManAtArms:'Man-at-Arms',
  TwoHandedSwordsman:'Two-Handed Swordsman', TrebuchetUnpacked:'Trebuchet (unpacked)'};
const unitName = kind => specialNames[kind] ?? kind.replace(/([a-z])([A-Z])/g, '$1 $2');
const commandHint = actor => {
  if (actor.kind==='Villager') return 'Right-click: work or move · Ctrl+right-click: queue move · Ctrl+0–9: group';
  if (actor.kind==='FishingShip') return 'Right-click: fish or move';
  if (actor.kind==='General') return 'Right-click ground: march with your army · Alt+right-click: General only';
  if (actor.kind==='Steward') return 'Garrison in any owned Town Center without a Steward';
  if (actor.kind==='Monk') return 'Right-click: convert or move · heals nearby injured units automatically';
  if (actor.kind==='TransportShip') return 'Load at the bank · sail · unload passengers';
  if (actor.kind==='Sheep') return 'Right-click: herd owned sheep';
  if (animals.includes(actor.kind)) return 'Select a villager to hunt this animal';
  return actor.stance?'Right-click: attack or move · Ctrl+0–9: group':'Right-click: move · Ctrl+0–9: group';
};
const show = (id,visible) => { set(byId(id), 'hidden', !visible); };

export function initInterface() {
  initBuildings();autoFit(byId('training-queue'),{high:30,low:22});
  initActivity();
  initSteward();
  initVassal();
  initGeneralPanel();
  byId('move-units').querySelector('.icon').outerHTML=packIcon('commands','move','move');
  byId('stop-units').querySelector('.icon').outerHTML=packIcon('commands','stop','stop');
  byId('delete-units').querySelector('.icon').outerHTML=packIcon('commands','delete','delete');
  initArt();
  const scene = () => byId('scene');
  const menu = byId('game-menu');
  const grid = byId('command-grid');
  Array.from({length:15}, () => {
    const node = document.createElement('span'); node.className='empty-command'; node.setAttribute('aria-hidden','true'); grid.append(node); return node;
  });
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
  let previousSelection='';
  const labels = actorLabels(icon);
  const update = state => {
    // Read style before changing any panels or actor labels.
    const bottom=parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--bottom'))||208;
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
    const selected=state.selected, count=selected.length, first=selected[0];
    const building=count>0 && selected.every(actor=>buildingKinds.includes(actor.kind));
    const owned=count>0 && selected.every(actor=>actor.owned);
    const units=owned && !building;
    const military=units && count>1 && selected.every(u=>u.formation);
    const stance=units && selected.every(u=>u.stance);
    show('combat-stance-label',stance);show('command-context',!stance);
    set(byId('combat-stance'), 'value', scene().dataset.selectedStance??'');
    for(const name of ['line','box','staggered','flank']){show(`formation-${name}`,military);}
    show('attack-move-units',units && selected.some(u=>u.attack>0 && u.hp>0));
    show('patrol-units',units && selected.some(u=>u.stance && u.hp>0));
    show('general-only-move',owned && count===1 && first.kind==='General');
    attribute(byId('selection'),'data-count',String(count));
    attribute(byId('selection'),'data-kind',first?.kind ?? '');
    attribute(byId('selection'),'data-owned',String(owned));
    attribute(byId('selection'),'data-villagers',String(owned && selected.some(unit=>unit.kind==='Villager')));
    attribute(byId('selection'),'data-ships',String(owned && selected.some(unit=>unit.kind==='FishingShip')));
    show('selection-empty',count===0); show('selection-content',count>0);
    show('train-villager',building && owned); show('move-units',units); show('stop-units',units);
    show('delete-units',owned && count>0);
    // The building menu sets placeholder visibility after all commands update.
    text('command-context',count>1?`${count} ${building?'buildings':'units'}`:first?.name ?? 'Select a unit');
    text('command-hint',!owned && first?'You cannot command this player’s units or buildings':
      building?'Q: train a villager · 50 food':units?commandHint(first):
      'Click to select · drag: group · arrows: camera · 0–9: recall group');
    show('selection-health',count===1);
    show('unit-hp',count===1 && !building); show('unit-combat',count===1 && !building);
    if (first) {
      text('selection-owner',first.player<0?'Wildlife':first.owned?'Your village':`Player ${first.player+1}`);
      text('unit-hp',`${first.hp} / ${first.maxHp} HP`);
      set(byId('selection-health'), 'value', first.hp); set(byId('selection-health'), 'max', Math.max(1,first.maxHp));
      text('unit-combat',`Attack ${first.attack} · Armour ${first.melee} / ${first.pierce} · Range ${first.range}`);
      const groupName=selected.every(unit=>unit.kind===first.kind)?(building&&first.name.endsWith('s')?first.name:building&&first.name.endsWith('y')?`${first.name.slice(0,-1)}ies`:`${first.name}s`):'units';
      text('selection-name',count>1?`${count} ${groupName} selected`:first.name);
      text('selection-task',count===1?building?(state.queue.length?'Training':'Building'): `${first.task}${first.resource && first.task==='Gathering'?' '+first.resource:''}`:'Right-click to give this group an order');
      text('selection-load',animals.includes(first.kind)?`${first.amount} food remaining`:count===1 && first.amount?`Carrying ${first.amount}/${first.capacity} ${first.resource}`:building?first.kind==='Castle'?'Supports 20 population':castleKinds.includes(first.kind)?'':'Resource drop-off · supports 5 population':'');
      const signature=selected.map(unit=>`${unit.id}:${unit.kind}`).join(',');
      if (signature!==previousSelection) {
        setEntityIcon(byId('selection-portrait'),first.kind,playerTint(first.player,first.owned,first.colour));
        renderSelectionIcons(byId('selected-icons'),count>1?selected.map(unit=>({id:unit.id,kind:unit.kind,name:unit.name,tint:playerTint(unit.player,unit.owned,unit.colour)})):[],entityIcon);
        previousSelection=signature;
      }
    } else previousSelection='';
    window.aoeBuildings?.queue(state.queue,building && owned,count>1?state.trainingRows:[]);
    attribute(scene(),'data-actors',JSON.stringify(state.actors));
    attribute(scene(),'data-resource-targets',JSON.stringify(state.resources));
    attribute(scene(),'data-village-art',state.art?'native':'markers');
    labels.health(state.actors.filter(a=>a.selected && a.hp>0 && !buildingKinds.includes(a.kind)));
    const onMap = a => a.hp>0 && a.x>0 && a.y>54 && a.y<innerHeight-bottom;
    labels.idle(state.actors.filter(a=>onMap(a) && a.owned && a.task==='Idle' && (a.kind==='Villager' || a.kind==='FishingShip')));
    labels.flags(state.actors.filter(a=>onMap(a) && a.garrison>0));
    labels.loads(state.actors.filter(a=>onMap(a) && a.load));
  };
  let roster = [];
  window.aoeInterface = {
    details: updateDetails,
    kinds(kinds,formation,stance) {
      const units = kinds.map((kind,i)=>({kind,name:unitName(kind),formation:!!formation[i],stance:!!stance[i]}));
      window.aoeBuildings.roster(units);
      roster = [...units, ...buildingKinds.map((kind,i)=>({kind,name:buildingNames[i],building:true})),
        ...animals.map(kind=>({kind,name:kind}))];
    },
    numeric(values,status) {
    const tasks=['Idle','Moving','Gathering','Carrying','Returning to drop-off','Attacking','Dying','Building','Farming','Hunting','Shepherding','Fishing','Seeding','Idle','Fleeing','Attacking','Dying','Carcass','Packing','Unpacking'];
    const resources=['food','wood','gold','stone',''];
    const state={stockpile:values.slice(0,4),population:values.slice(4,6),workers:values.slice(6,10),seconds:values[10],art:values[11]===1,status,actors:[],selected:[],queue:[],resources:[]};
    state.idleVillagers=values[15];
    state.idleMilitary=values[16];
    let at=17;
    for (let index=0;index<values[12];index++,at+=19) {
      const [id,kind,x,y,selected,task,resource,amount,hp,maxHp,attack,melee,pierce,head,capacity,range,player,owned,colour]=values.subarray(at,at+19);
      const actor = roster[kind];
      const taskName = tasks[task];
      const label = taskName==='Farming' && actor.kind==='FishingShip' ? 'Fishing' : taskName;
      state.actors.push({id,player,owned:!!owned,kind:actor.kind,x,y,selected:!!selected,hp,maxHp,head,task:label,load:actor.kind==='Villager' && amount?[resources[resource],amount]:null,garrison:owned&&actor.building?capacity:0});
      if (selected) state.selected.push({...actor,id,player,owned:!!owned,colour,task:label,resource:resources[resource],amount,hp,maxHp,attack,melee,pierce,capacity,range});
    }
    for (let index=0;index<values[13];index++,at+=3) state.queue.push([values[at],values[at+1],values[at+2]]);
    for (let index=0;index<values[14];index++,at+=4) state.resources.push({kind:values[at],x:values[at+1],y:values[at+2],distance:values[at+3]});
    state.trainingRows=[];
    while(at<values.length) {
      const [id,kind,length]=values.subarray(at,at+3);
      at+=3;
      state.trainingRows.push([id,kind,Array.from(values.subarray(at,at+length))]);
      at+=length;
    }
    update(state);
    window.aoeBuildings?.refresh();
  }};
}
