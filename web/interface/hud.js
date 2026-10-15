// Original interface artwork and DOM presentation. Authoritative values arrive
// from Rust at most four times per second; no frame loop or gameplay rules here.
const icon = name => `<svg class="icon" aria-hidden="true"><use href="/interface/icons.svg#${name}"></use></svg>`;
const symbol = kind => ({ Scout:'scout', Villager:'villager', TownCenter:'towncenter' })[kind] ?? 'villager';
const byId = id => document.getElementById(id);
const text = (id,value) => { const node = byId(id); const next = String(value); if (node && node.textContent !== next) node.textContent = next; };
const show = (id,visible) => { byId(id).hidden = !visible; };
const attribute = (node,name,value) => { if (node.getAttribute(name) !== value) node.setAttribute(name,value); };

export function initInterface() {
  const scene = byId('scene');
  const menu = byId('game-menu');
  const grid = byId('command-grid');
  const empty = Array.from({length:15}, () => {
    const node = document.createElement('span'); node.className='empty-command'; node.setAttribute('aria-hidden','true'); grid.append(node); return node;
  });
  byId('menu-toggle').addEventListener('click', () => {
    window.dispatchEvent(new Event('blur'));
    menu.showModal();
  });
  byId('resume-game').addEventListener('click', () => { menu.close(); scene.focus(); });
  byId('quit-game').addEventListener('click', () => {
    if (byId('start-menu')) location.assign('/'); else location.reload();
  });
  byId('reconnect').addEventListener('click', () => { menu.close(); byId('map-creator').hidden=true; });
  byId('creator-close').addEventListener('click', () => { byId('map-creator').hidden=true; scene.focus(); });
  let drag = null;
  const box = byId('drag-box');
  scene.addEventListener('pointerdown', event => { if (event.button===0) drag=[event.clientX,event.clientY]; });
  scene.addEventListener('pointermove', event => {
    if (!drag) return;
    const width=Math.abs(event.clientX-drag[0]), height=Math.abs(event.clientY-drag[1]);
    box.hidden = width<4 && height<4;
    Object.assign(box.style,{left:`${Math.min(drag[0],event.clientX)}px`,top:`${Math.min(drag[1],event.clientY)}px`,width:`${width}px`,height:`${height}px`});
  });
  for (const event of ['pointerup','pointercancel','lostpointercapture']) scene.addEventListener(event,()=>{ drag=null;box.hidden=true; });
  let previousSelection='', previousQueue='', previousLoads='';
  window.aoeInterface = { update(state) {
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
    attribute(scene,'data-actors',JSON.stringify(state.actors));
    attribute(scene,'data-resource-targets',JSON.stringify(state.resources));
    attribute(scene,'data-village-art',state.art?'native':'markers');
    const loads=state.actors.filter(actor=>actor.load && actor.x>0 && actor.y>54 && actor.y<innerHeight-208);
    const signature=JSON.stringify(loads);
    if (signature!==previousLoads) {
      byId('carried-loads').innerHTML=loads.map(actor=>`<span class="load-badge" style="left:${actor.x+12}px;top:${actor.y-36}px">${icon(actor.load[0])}${actor.load[1]}</span>`).join('');previousLoads=signature;
    }
  }};
}
