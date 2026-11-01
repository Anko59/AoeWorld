import { castleUnits, castleUnitNames, castleKinds } from '/interface/castle.js';
import { initBuildings, buildingKinds, buildingNames } from '/interface/buildings.js';
import { initArt, icon as packIcon, entityIcon, setEntityIcon } from '/interface/ui-art.js';
import { initVassal } from '/interface/vassal/panel.js';
import { updateDetails } from '/interface/details.js';
// Original interface artwork and DOM presentation. Authoritative values arrive
// from Rust at most four times per second; no frame loop or gameplay rules here.
const icon = name => packIcon('resources',name,name);
const byId = id => document.getElementById(id);
const text = (id,value) => { const node = byId(id); const next = String(value); if (node && node.textContent !== next) node.textContent = next; };
const show = (id,visible) => { byId(id).hidden = !visible; };
const attribute = (node,name,value) => { if (node.getAttribute(name) !== value) node.setAttribute(name,value); };

export function initInterface() {
  initBuildings();
  initVassal();
  byId('move-units').querySelector('.icon').outerHTML=packIcon('commands','move','move');
  byId('stop-units').querySelector('.icon').outerHTML=packIcon('commands','stop','stop');
  initArt();
  const scene = () => byId('scene');
  const menu = byId('game-menu');
  const grid = byId('command-grid');
  const empty = Array.from({length:15}, () => {
    const node = document.createElement('span'); node.className='empty-command'; node.setAttribute('aria-hidden','true'); grid.append(node); return node;
  });
  byId('menu-toggle').addEventListener('click', () => {
    window.dispatchEvent(new Event('blur'));
    menu.showModal();
  });
  byId('resume-game').addEventListener('click', () => { menu.close(); scene().focus(); });
  byId('quit-game').addEventListener('click', () => { menu.close(); window.aoeStart.menu(); });
  byId('reconnect').addEventListener('click', () => menu.close());
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
  let previousSelection='', previousLoads='', previousHealth='', previousIdle='', previousFlags='';
  const update = state => {
    ['wood','food','gold','stone'].forEach((resource,index) => {
      text(`stockpile-${resource}`,state.stockpile[index]); text(`workers-${resource}`,state.workers[index]);
    });
    text('population',state.population);
    text('idle-villager-count',state.idleVillagers);
    byId('idle-villagers').disabled=state.idleVillagers===0;
    byId('idle-villagers').classList.toggle('has-idle',state.idleVillagers>0);
    text('idle-military-count',state.idleMilitary);
    byId('idle-military').disabled=state.idleMilitary===0;
    byId('idle-military').classList.toggle('has-idle',state.idleMilitary>0);
    const minutes=Math.floor(state.seconds/60), seconds=state.seconds%60;
    text('game-clock',`${String(minutes).padStart(2,'0')}:${String(seconds).padStart(2,'0')}`);
    text('connection',state.status);
    byId('game-notice').classList.toggle('connected',state.status==='connected');
    const selected=state.selected, count=selected.length, first=selected[0];
    const building=count>0 && selected.every(actor=>buildingKinds.includes(actor.kind));
    const owned=count>0 && selected.every(actor=>actor.owned);
    const units=owned && !building;
    const military=units && count>1 && selected.every(u=>['Militia','Scout','Archer','Skirmisher','Spearman','ManAtArms',...castleUnits].includes(u.kind));
    const stance=units && selected.every(u=>!['Villager','FishingShip','TransportShip','Monk','Vassal','Sheep','Deer','Boar','Wolf'].includes(u.kind));
    show('combat-stance-label',stance);show('command-context',!stance);
    byId('combat-stance').value=scene().dataset.selectedStance??'';
    for(const name of ['line','box','staggered','flank']){show(`formation-${name}`,military);}
    show('attack-move-units',military);
    attribute(byId('selection'),'data-count',String(count));
    attribute(byId('selection'),'data-kind',first?.kind ?? '');
    attribute(byId('selection'),'data-owned',String(owned));
    attribute(byId('selection'),'data-villagers',String(owned && selected.some(unit=>unit.kind==='Villager')));
    show('selection-empty',count===0); show('selection-content',count>0);
    show('train-villager',building && owned); show('move-units',units); show('stop-units',units);
    const actions=building?1:units?2:0;
    empty.forEach((node,index)=>{node.hidden=index>=15-actions;});
    text('command-context',count>1?`${count} ${building?'buildings':'units'}`:first?.name ?? 'Select a unit');
    text('command-hint',!owned && first?'You cannot command this player’s units or buildings':building?'Q: train a villager · 50 food':units?first.kind==='Villager'?'Right-click: work or move · Ctrl+right-click: queue move · Ctrl+0–9: group':first.kind==='FishingShip'?'Right-click: fish or move':first.kind==='General'?'Right-click ground: march with your army · retreat and regroup automatically':first.kind==='Vassal'?'Garrison in its home Castle · 100-tile leash':first.kind==='Monk'?'Right-click: move · heals nearby injured units automatically':['Militia','Scout','Archer','Skirmisher','Spearman','ManAtArms',...castleUnits,'Galley','FireShip','DemolitionShip','TransportShip','WarGalley'].includes(first.kind)?'Right-click: attack or move · Ctrl+0–9: group':first.kind==='TransportShip'?'Load at the bank · sail · unload passengers':first.kind==='Sheep'?'Right-click: herd owned sheep':'Select a villager to hunt this animal':'Click to select · drag: group · arrows: camera · 0–9: recall group');
    show('selection-health',count===1);
    show('unit-hp',count===1 && !building); show('unit-combat',count===1 && !building);
    if (first) {
      text('selection-owner',first.player<0?'Wildlife':first.owned?'Your village':`Player ${first.player+1}`);
      text('unit-hp',`${first.hp} / ${first.maxHp} HP`);
      byId('selection-health').value=first.hp; byId('selection-health').max=Math.max(1,first.maxHp);
      text('unit-combat',`Attack ${first.attack} · Armour ${first.melee} / ${first.pierce} · Range ${first.range}`);
      const groupName=selected.every(unit=>unit.kind===first.kind)?(building&&first.name.endsWith('s')?first.name:building&&first.name.endsWith('y')?`${first.name.slice(0,-1)}ies`:`${first.name}s`):'units';
      text('selection-name',count>1?`${count} ${groupName} selected`:first.name);
      text('selection-task',count===1?building?(state.queue.length?'Training':'Building'): `${first.task}${first.resource && first.task==='Gathering'?' '+first.resource:''}`:'Right-click to give this group an order');
      text('selection-load',['Sheep','Deer','Boar','Wolf'].includes(first.kind)?`${first.amount} food remaining`:count===1 && first.amount?`Carrying ${first.amount}/${first.capacity} ${first.resource}`:building?first.kind==='Castle'?'Trains leaders and military units':castleKinds.includes(first.kind)?'':'Resource drop-off · trains villagers':'');
      const signature=selected.map(unit=>`${unit.id}:${unit.kind}`).join(',');
      if (signature!==previousSelection) {
        setEntityIcon(byId('selection-portrait'),first.kind);
        byId('selected-icons').innerHTML=count>1?selected.map(unit=>`<span class="unit-token" title="${unit.name}">${entityIcon(unit.kind)}</span>`).join(''):'';
        previousSelection=signature;
      }
    } else previousSelection='';
    window.aoeBuildings?.queue(state.queue,building && owned);
    attribute(scene(),'data-actors',JSON.stringify(state.actors));
    attribute(scene(),'data-resource-targets',JSON.stringify(state.resources));
    attribute(scene(),'data-village-art',state.art?'native':'markers');
    const health=state.actors.filter(a=>a.selected && a.hp>0 && !buildingKinds.includes(a.kind)).map(a=>`<span class="unit-hp-bar" style="left:${a.x-18}px;top:${a.head}px"><span style="width:${100*a.hp/a.maxHp}%"></span></span>`).join('');
    if (health!==previousHealth) { byId('unit-health-bars').innerHTML=health; previousHealth=health; }
    // Own idle workers carry a marker so they can be found on the map.
    const idle=state.actors.filter(a=>a.owned && a.hp>0 && a.task==='Idle' && (a.kind==='Villager' || a.kind==='FishingShip') && a.x>0 && a.y>54 && a.y<innerHeight-208).map(a=>`<span class="idle-marker" style="left:${a.x-6}px;top:${a.head-16}px">z</span>`).join('');
    if (idle!==previousIdle) { byId('idle-markers').innerHTML=idle; previousIdle=idle; }
    const flags=state.actors.filter(actor=>actor.hp>0 && actor.garrison>0 && actor.x>0 && actor.y>54 && actor.y<innerHeight-208).map(actor=>`<span class="garrison-flag" data-building-id="${actor.id}" title="${actor.garrison} garrisoned" style="left:${actor.x}px;top:${actor.y-48}px">${actor.garrison}</span>`).join('');
    if (flags!==previousFlags) { byId('garrison-flags').innerHTML=flags; previousFlags=flags; }
    const loads=state.actors.filter(actor=>actor.hp>0 && actor.load && actor.x>0 && actor.y>54 && actor.y<innerHeight-208);
    const signature=JSON.stringify(loads);
    if (signature!==previousLoads) {
      byId('carried-loads').innerHTML=loads.map(actor=>`<span class="load-badge" style="left:${actor.x+12}px;top:${actor.y-36}px">${icon(actor.load[0])}${actor.load[1]}</span>`).join('');previousLoads=signature;
    }
  };
  window.aoeInterface = { details: updateDetails, numeric(values,status) {
    const kinds=['Scout','Villager',...buildingKinds,'Militia','FishingShip','Sheep','Deer','Boar','Wolf','Archer','Skirmisher','Spearman','ManAtArms',...castleUnits,'Galley','FireShip','DemolitionShip','TransportShip','WarGalley','Vassal','General'], names=['Scout Cavalry','Villager',...buildingNames,'Militia','Fishing Ship','Sheep','Deer','Boar','Wolf','Archer','Skirmisher','Spearman','Man-at-Arms',...castleUnitNames,'Galley','Fire Ship','Demolition Ship','Transport Ship','War Galley','Vassal','General'];
    const tasks=['Idle','Moving','Gathering','Carrying','Returning to drop-off','Attacking','Dying','Building','Farming','Hunting','Shepherding','Fishing','Idle','Fleeing','Attacking','Dying','Carcass'];
    const resources=['food','wood','gold','stone',''];
    const state={stockpile:values.slice(0,4),population:values[4],workers:values.slice(5,9),seconds:values[9],art:values[10]===1,status,actors:[],selected:[],queue:[],resources:[]};
    state.idleVillagers=values[14];
    state.idleMilitary=values[15];
    let at=16;
    for (let index=0;index<values[11];index++,at+=18) {
      const [id,kind,x,y,selected,task,resource,amount,hp,maxHp,attack,melee,pierce,head,capacity,range,player,owned]=values.subarray(at,at+18);
      state.actors.push({id,player,owned:!!owned,kind:kinds[kind],x,y,selected:!!selected,hp,maxHp,head,task:tasks[task],load:kind===1 && amount?[resources[resource],amount]:null,garrison:owned&&buildingKinds.includes(kinds[kind])?capacity:0});
      if (selected) state.selected.push({id,player,owned:!!owned,kind:kinds[kind],name:names[kind],task:tasks[task],resource:resources[resource],amount,hp,maxHp,attack,melee,pierce,capacity,range});
    }
    for (let index=0;index<values[12];index++,at+=3) state.queue.push([values[at],values[at+1],values[at+2]]);
    for (let index=0;index<values[13];index++,at+=4) state.resources.push({kind:values[at],x:values[at+1],y:values[at+2],distance:values[at+3]});
    update(state);
    window.aoeBuildings?.refresh();
  }};
}
