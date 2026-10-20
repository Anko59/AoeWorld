// DOM presentation for authoritative Rust building state and footprint previews.
const byId = id => document.getElementById(id);
export const buildingKinds = ['TownCenter','House','LumberCamp','MiningCamp','Mill','Farm','Barracks','PalisadeWall','Outpost'];
export const buildingNames = ['Town Center','House','Lumber Camp','Mining Camp','Mill','Farm','Barracks','Palisade Wall','Outpost'];
const commands = [
  ['house','House','Q','25 wood'], ['lumber-camp','Lumber Camp','E','100 wood'],
  ['mining-camp','Mining Camp','R','100 wood'], ['mill','Mill','W','100 wood'],
  ['farm','Farm','S','60 wood · needs Mill'], ['barracks','Barracks','A','175 wood'],
  ['palisade-wall','Palisade Wall','D','2 wood / tile'], ['outpost','Outpost','F','25 wood · 5 stone'],
];
export function initBuildings() {
  const grid = byId('command-grid');
  const build = document.createElement('button');
  build.id='build-menu'; build.hidden=true; build.innerHTML='⌂<kbd>B</kbd><small>Build</small>'; build.title='Build menu (B)'; grid.append(build);
  const buttons=commands.map(([id,name,key,cost])=>{
    const button=document.createElement('button'); button.id=`build-${id}`; button.className="build-command"; button.hidden=true;
    button.title=`${name} · ${cost} · ${key}`; button.setAttribute('aria-label',button.title);
    button.innerHTML=`<span>${name}</span><kbd>${key}</kbd><small>${cost}</small>`; grid.append(button); return button;
  });
  const research=['Loom · 50 gold','Town Watch · 75 food','Feudal Age · 500 food'];
  const techs=['loom','town-watch','feudal-age'].map((id,i)=>{
    const button=document.createElement('button'); button.id=`research-${id}`;button.className="research-command";button.hidden=true;button.textContent=research[i];grid.append(button);return button;
  });
  const panel=document.createElement('div');panel.id='building-panel';panel.hidden=true;
  panel.innerHTML='<b id="building-hp"></b><div id="building-status"></div><progress id="construction-progress" max="1000"></progress><div id="research-status"></div><button id="cancel-construction" hidden>Cancel construction</button><button id="cancel-research" hidden>Cancel research</button>';
  byId('selection-content').insertBefore(panel,byId('training'));
  const progressLabels=document.createElement('div');progressLabels.id='construction-labels';document.body.append(progressLabels);
  const preview=document.createElementNS('http://www.w3.org/2000/svg','svg');preview.id='building-preview';preview.setAttribute('aria-hidden','true');document.body.append(preview);
  let open=false, state=[], lastPreview='';
  const menu=()=>{
    const villager=byId('selection').dataset.villagers==='true';
    build.hidden=!villager; if(!villager)open=false;
    buttons.forEach((b,i)=>{b.hidden=!villager||!open;b.disabled=state[11+i]!==1;});
    if(open){for(const id of ['move-units','stop-units'])byId(id).hidden=true;byId('command-context').textContent='Build';byId('command-hint').textContent=byId('scene').dataset.orderMode==='build'?'Left-click a green footprint · drag walls · Esc cancels':'Choose a building · Esc closes the menu';}
    grid.classList.toggle('building-menu-open',open);
    build.setAttribute('aria-expanded',String(open));
    const active=Array.from(grid.children).filter(n=>n.tagName==='BUTTON'&&!n.hidden).length;
    grid.querySelectorAll('.empty-command').forEach((n,i)=>{n.hidden=i>=Math.max(0,15-active);});
  };
  build.addEventListener('click',()=>{open=!open;menu();});
  window.aoeBuildings={
    key(event) {
      const key=event.key.toUpperCase();
      if(key==='B'&&!build.hidden){build.click();event.preventDefault();return true;}
      if(key==='ESCAPE'&&open){open=false;menu();return false;}
      const index=commands.findIndex(c=>c[2]===key);
      if(open&&index>=0){buttons[index].click();event.preventDefault();return true;}
      return false;
    },
    refresh:menu,
    update(v) {
      state=Array.from(v);menu();
      byId('age').textContent=v[0]===1?'II  Feudal Age':'I  Dark Age';
      const kind=v[1],selected=kind>=0;
      panel.hidden=!selected;
      techs.forEach((b,i)=>{b.hidden=kind!==0;b.disabled=v[8+i]!==1||byId('observer-label').hidden===false;});
      const train=byId('train-villager');
      if(selected){train.hidden=![0,6].includes(kind)||v[4]<1000;train.disabled=v[20]!==1||!byId('observer-label').hidden;train.setAttribute('aria-label',kind===6?'Train Militia, 60 food, 20 gold, Q':'Train Villager, 50 food, Q');train.title=train.getAttribute('aria-label');train.querySelector('.cost').textContent=kind===6?'60 F · 20 G':'50';}
      progressLabels.innerHTML=Array.from({length:v[21]??0},(_,i)=>{const at=22+i*3;return `<span style="left:${v[at]}px;top:${v[at+1]}px">${Math.floor(v[at+2]/10)}%<progress max="1000" value="${v[at+2]}"></progress></span>`;}).join('');
      if(selected){byId('command-hint').textContent=v[4]<1000?'Right-click this foundation with villagers to resume construction':kind===0?'Q: train a villager · research below':kind===6?'Q: train Militia · 60 food, 20 gold':kind===5?'Right-click this Farm with a villager to work it':'Select villagers to gather or build';byId('selection-task').textContent=v[4]<1000?'Under construction':v[6]>=0?'Researching':v[19]===0&&byId('training').hidden===false?'Training Villager':kind===6&&byId('training').hidden===false?'Training Militia':'Ready';byId('selection-load').textContent=['Resource drop-off · supports 5 population','Supports 5 population','Wood drop-off','Gold and stone drop-off','Food drop-off','Food production','Trains Militia','Blocks movement','Scouts the surrounding area'][kind];}
      byId('building-hp').textContent=`${v[2]} / ${v[3]} HP`;
      byId('building-status').textContent=v[4]<1000?`Constructing · ${Math.floor(v[4]/10)}%`:v[5]>=0?`Complete · ${v[5]} food remaining`:'Complete';
      byId('construction-progress').hidden=v[4]>=1000;byId('construction-progress').value=v[4];
      byId('cancel-construction').hidden=!selected||v[4]>=1000;
      byId('cancel-research').hidden=v[6]<0;
      byId('research-status').textContent=v[6]<0?'':`${research[v[6]].split(' · ')[0]} · ${Math.floor(v[7]*100)}%`;
      if(kind===6&&byId('training-label').textContent.includes('Villager'))byId('training-label').textContent=byId('training-label').textContent.replace('Villager','Militia');
      menu();
    },
    preview(v) {
      const signature=Array.from(v).join(','); if(signature===lastPreview)return; lastPreview=signature;
      preview.replaceChildren();preview.hidden=v.length===0;
      byId('scene').dataset.placementValid=String(v.length>0&&v[0]===1);
      for(let i=1;i+7<v.length;i+=8){
        const polygon=document.createElementNS(preview.namespaceURI,'polygon');
        polygon.setAttribute('points',`${v[i]},${v[i+1]} ${v[i+2]},${v[i+3]} ${v[i+4]},${v[i+5]} ${v[i+6]},${v[i+7]}`);
        polygon.setAttribute('class',v[0]===1?'valid':'invalid');preview.append(polygon);
      }
    },
  };
}
