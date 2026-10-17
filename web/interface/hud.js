// Original interface artwork and DOM presentation. Authoritative values arrive
// from Rust at most four times per second; no frame loop or gameplay rules here.
const icon = name => `<svg class="icon" aria-hidden="true"><use href="/interface/icons.svg#${name}"></use></svg>`;
const symbol = kind => ({ Scout:'scout', Villager:'villager', TownCenter:'towncenter' })[kind] ?? 'villager';
const byId = id => document.getElementById(id);
const text = (id,value) => { const node = byId(id); const next = String(value); if (node && node.textContent !== next) node.textContent = next; };
const show = (id,visible) => { byId(id).hidden = !visible; };
const attribute = (node,name,value) => { if (node.getAttribute(name) !== value) node.setAttribute(name,value); };

export function initInterface() {
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
  let previousSelection='', previousQueue='', previousLoads='';
  const update = state => {
    ['wood','food','gold','stone'].forEach((resource,index) => {
      text(`stockpile-${resource}`,state.stockpile[index]); text(`workers-${resource}`,state.workers[index]);
    });
    text('population',state.population.join('/'));
    const minutes=Math.floor(state.seconds/60), seconds=state.seconds%60;
    text('game-clock',`${String(minutes).padStart(2,'0')}:${String(seconds).padStart(2,'0')}`);
    text('connection',state.status);
    byId('game-notice').classList.toggle('connected',state.status==='connected');
    const selected=state.selected, count=selected.length, first=selected[0];
    const building=count===1 && first.kind==='TownCenter';
    const units=count>0 && !building;
    attribute(byId('selection'),'data-count',String(count));
    attribute(byId('selection'),'data-kind',first?.kind ?? '');
    show('selection-empty',count===0); show('selection-content',count>0);
    show('train-villager',building); show('move-units',units); show('stop-units',units);
    for (const id of ['train-villager','move-units','stop-units','cancel-training']) byId(id).disabled=state.observer;
    const actions=building?1:units?2:0;
    empty.forEach((node,index)=>{node.hidden=index>=15-actions;});
    text('command-context',count>1?`${count} units`:first?.name ?? 'Select a unit');
    text('command-hint',building?'Q: train a villager · 50 food':units?'Right-click: gather or move · double-click: same type':'Click to select · drag to select a group');
    show('observer-label',state.observer);
    if (first) {
      const groupName=selected.every(unit=>unit.kind===first.kind)?`${first.name}s`:'units';
      text('selection-name',count>1?`${count} ${groupName} selected`:first.name);
      text('selection-task',count===1?`${first.task}${first.resource && first.task==='Gathering'?' '+first.resource:''}`:'Right-click to give this group an order');
      text('selection-load',count===1 && first.amount?`Carrying ${first.amount}/10 ${first.resource}`:building?'Resource drop-off · supports 5 population':'');
      const signature=selected.map(unit=>`${unit.id}:${unit.kind}`).join(',');
      if (signature!==previousSelection) {
        byId('selection-portrait').innerHTML=icon(symbol(first.kind));
        byId('selected-icons').innerHTML=count>1?selected.map(unit=>`<span class="unit-token" title="${unit.name}">${icon(symbol(unit.kind))}</span>`).join(''):'';
        previousSelection=signature;
      }
    } else previousSelection='';
    const queue=state.queue;
    show('training',building && queue.length>0);
    if (queue.length) {
      const percent=Math.floor(queue[0][0]/Math.max(1,queue[0][1])*100);
      byId('training-progress').value=percent;
      text('training-label',`Training Villager · ${percent}%`);
      const signature=String(queue.length);
      if (signature!==previousQueue) {
        byId('training-queue').innerHTML=queue.map(()=>`<span class="unit-token" title="Villager queued">${icon('villager')}</span>`).join('');previousQueue=signature;
      }
    } else previousQueue='';
    attribute(scene(),'data-actors',JSON.stringify(state.actors));
    attribute(scene(),'data-resource-targets',JSON.stringify(state.resources));
    attribute(scene(),'data-village-art',state.art?'native':'markers');
    const loads=state.actors.filter(actor=>actor.load && actor.x>0 && actor.y>54 && actor.y<innerHeight-208);
    const signature=JSON.stringify(loads);
    if (signature!==previousLoads) {
      byId('carried-loads').innerHTML=loads.map(actor=>`<span class="load-badge" style="left:${actor.x+12}px;top:${actor.y-36}px">${icon(actor.load[0])}${actor.load[1]}</span>`).join('');previousLoads=signature;
    }
  };
  window.aoeInterface = { connection(value) { scene().dataset.connected=String(value); }, numeric(values,status) {
    const kinds=['Scout','Villager','TownCenter'], names=['Scout','Villager','Town Center'];
    const tasks=['Idle','Moving','Gathering','Carrying','Returning to Town Center','Ready to train','Training Villager'];
    const resources=['food','wood','gold','stone',''];
    const state={stockpile:values.slice(0,4),population:values.slice(4,6),workers:values.slice(6,10),seconds:values[10],observer:values[11]===1,art:values[12]===1,status,actors:[],selected:[],queue:[],resources:[]};
    let at=16;
    for (let index=0;index<values[13];index++,at+=8) {
      const [id,kind,x,y,selected,task,resource,amount]=values.subarray(at,at+8);
      state.actors.push({id,kind:kinds[kind],x,y,selected:!!selected,load:amount?[resources[resource],amount]:null});
      if (selected) state.selected.push({id,kind:kinds[kind],name:names[kind],task:tasks[task],resource:resources[resource],amount});
    }
    for (let index=0;index<values[14];index++,at+=2) state.queue.push([values[at],values[at+1]]);
    for (let index=0;index<values[15];index++,at+=4) state.resources.push({kind:values[at],x:values[at+1],y:values[at+2],distance:values[at+3]});
    update(state);
  }};
}
