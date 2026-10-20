// DOM presentation for authoritative Rust building state and footprint previews.
const byId = id => document.getElementById(id);
export const buildingKinds = ['TownCenter','House','LumberCamp','MiningCamp','Mill','Farm','Barracks','PalisadeWall','Outpost','Dock','ArcheryRange','Stable','Blacksmith','Market','WatchTower','PalisadeGate'];
export const buildingNames = ['Town Center','House','Lumber Camp','Mining Camp','Mill','Farm','Barracks','Palisade Wall','Outpost','Dock','Archery Range','Stable','Blacksmith','Market','Watch Tower','Palisade Gate'];
const commands = [
  ['house','House','Q','25 wood'], ['lumber-camp','Lumber Camp','R','100 wood'],
  ['mining-camp','Mining Camp','E','100 wood'], ['mill','Mill','W','100 wood'],
  ['farm','Farm','A','60 wood · needs Mill'], ['barracks','Barracks','Q','175 wood'],
  ['palisade-wall','Palisade Wall','S','2 wood / tile'], ['outpost','Outpost','A','25 wood · 5 stone'],
  ['dock','Dock','T','150 wood · needs a coast'],
  ['archery-range','Archery Range','W','175 wood · needs Barracks'], ['stable','Stable','E','175 wood · needs Barracks'],
  ['blacksmith','Blacksmith','S','150 wood'], ['market','Market','D','175 wood · needs Mill'],
  ['watch-tower','Watch Tower','F','35 wood · 125 stone'], ['palisade-gate','Palisade Gate','X','30 wood'],
];
export function initBuildings() {
  const grid = byId('command-grid');
  const build = document.createElement('button');
  build.id='build-menu'; build.hidden=true; build.innerHTML='⌂<kbd>B</kbd><small>Economic</small>'; build.title='Economic buildings (B)'; grid.append(build);
  const military=document.createElement('button');military.id='build-military-menu';military.hidden=true;military.innerHTML='⚑<kbd>V</kbd><small>Military</small>';military.title='Military buildings (V)';grid.append(military);
  const militaryKinds=new Set([5,6,7,9,10,13,14]);
  const category=i=>militaryKinds.has(i)?'military':'economy';
  const buttons=commands.map(([id,name,key,cost])=>{
    const button=document.createElement('button'); button.id=`build-${id}`; button.className="build-command"; button.hidden=true;
    button.title=`${name} · ${cost} · ${key}`; button.setAttribute('aria-label',button.title);
    button.innerHTML=`<span>${name}</span><kbd>${key}</kbd><small>${cost}</small>`; grid.append(button); return button;
  });
  const research=['Loom · 50 gold','Town Watch · 75 food','Feudal Age · 500 food','Double-Bit Axe · 100 food · 50 wood','Horse Collar · 75 food · 75 wood','Gold Mining · 100 food · 75 wood','Stone Mining · 100 food · 75 wood','Wheelbarrow · 175 food · 50 wood'];
  const sites=[0,0,0,2,4,3,3,0];
  const buildStart=8+research.length, trainKind=buildStart+commands.length, trainEnabled=trainKind+1, foundationCount=trainKind+2;
  const techs=['loom','town-watch','feudal-age','double-bit-axe','horse-collar','gold-mining','stone-mining','wheelbarrow'].map((id,i)=>{
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
    build.hidden=!villager;military.hidden=!villager;if(!villager)open=false;
    buttons.forEach((b,i)=>{b.hidden=!villager||open!==category(i);b.disabled=state[buildStart+i]!==1;const [id,name,key,cost]=commands[i];b.title=`${name} · ${cost} · ${category(i)==='military'?'V':'B'} ${key}${state[0]===0&&i>=9&&i<=13?' · requires Feudal Age':''}`;b.setAttribute('aria-label',b.title);});
    if(open){for(const id of ['move-units','stop-units'])byId(id).hidden=true;byId('command-context').textContent=open==='military'?'Military buildings':'Economic buildings';byId('command-hint').textContent=byId('scene').dataset.orderMode==='build'?'Left-click a green footprint · drag walls · Esc cancels':'Choose a building · Esc closes the menu';}
    grid.classList.toggle('building-menu-open',Boolean(open));
    build.setAttribute('aria-expanded',String(open==='economy'));military.setAttribute('aria-expanded',String(open==='military'));
    const active=Array.from(grid.children).filter(n=>n.tagName==='BUTTON'&&!n.hidden).length;
    grid.querySelectorAll('.empty-command').forEach((n,i)=>{n.hidden=i>=Math.max(0,15-active);});
  };
  build.addEventListener('click',()=>{open=open==='economy'?false:'economy';menu();});
  military.addEventListener('click',()=>{open=open==='military'?false:'military';menu();});
  window.aoeBuildings={
    key(event) {
      const key=event.key.toUpperCase();
      if(key==='B'&&!build.hidden){build.click();event.preventDefault();return true;}
      if(key==='V'&&!military.hidden){military.click();event.preventDefault();return true;}
      if(key==='ESCAPE'&&open){open=false;menu();return false;}
      const index=commands.findIndex((c,i)=>category(i)===open&&c[2]===key);
      if(open&&index>=0){buttons[index].click();event.preventDefault();return true;}
      return false;
    },
    refresh:menu,
    update(v) {
      state=Array.from(v);menu();
      byId('age').textContent=v[0]===1?'II  Feudal Age':'I  Dark Age';
      const kind=v[1],selected=kind>=0;
      panel.hidden=!selected;
      techs.forEach((b,i)=>{b.hidden=kind!==sites[i];b.disabled=v[8+i]!==1||byId('observer-label').hidden===false;b.title=research[i]+(v[0]===0&&(i===1||i>=3)?' · requires Feudal Age':'');});
      const train=byId('train-villager');
      if(selected){train.hidden=![0,6,9].includes(kind)||v[4]<1000;train.disabled=v[trainEnabled]!==1||!byId('observer-label').hidden;train.setAttribute('aria-label',kind===9?'Train Fishing Ship, 75 wood, Q':kind===6?'Train Militia, 60 food, 20 gold, Q':'Train Villager, 50 food, Q');train.title=train.getAttribute('aria-label');train.querySelector('.cost').textContent=kind===9?'75 W':kind===6?'60 F · 20 G':'50';}
      progressLabels.innerHTML=Array.from({length:v[foundationCount]??0},(_,i)=>{const at=foundationCount+1+i*3;return `<span style="left:${v[at]}px;top:${v[at+1]}px">${Math.floor(v[at+2]/10)}%<progress max="1000" value="${v[at+2]}"></progress></span>`;}).join('');
      if(selected){byId('command-hint').textContent=v[4]<1000?'Right-click this foundation with villagers to resume construction':kind===0?'Q: train a villager · research below':kind===9?'Q: train a Fishing Ship · 75 wood':kind===6?'Q: train Militia · 60 food, 20 gold':kind===5?'Right-click this Farm with a villager to work it':'Select villagers to gather or build';byId('selection-task').textContent=v[4]<1000?'Under construction':v[6]>=0?'Researching':v[trainKind]===0&&byId('training').hidden===false?'Training Villager':kind===9&&byId('training').hidden===false?'Training Fishing Ship':kind===6&&byId('training').hidden===false?'Training Militia':'Ready';byId('selection-load').textContent=['Resource drop-off · supports 5 population','Supports 5 population','Wood drop-off','Gold and stone drop-off','Food drop-off','Food production','Trains Militia','Blocks movement','Scouts the surrounding area','Fish drop-off · trains Fishing Ships','Feudal military building · units follow later','Feudal military building · units follow later','Feudal blacksmith · upgrades follow later','Market · trading follows later','Arrow defence · 5 pierce attack · range 1–8','Opens for nearby friendly units'][kind];}
      byId('building-hp').textContent=`${v[2]} / ${v[3]} HP`;
      byId('building-status').textContent=v[4]<1000?`Constructing · ${Math.floor(v[4]/10)}%`:v[5]>=0?`Complete · ${v[5]} food remaining`:'Complete';
      byId('construction-progress').hidden=v[4]>=1000;byId('construction-progress').value=v[4];
      byId('cancel-construction').hidden=!selected||v[4]>=1000;
      byId('cancel-research').hidden=v[6]<0;
      byId('research-status').textContent=v[6]<0?'':`${research[v[6]].split(' · ')[0]} · ${Math.floor(v[7]*100)}%`;
      if([6,9].includes(kind)&&byId('training-label').textContent.includes('Villager'))byId('training-label').textContent=byId('training-label').textContent.replace('Villager',kind===9?'Fishing Ship':'Militia');
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
