import { transportLoads } from '/interface/navy/indicators.js';
import { initGarrison } from '/interface/garrison/panel.js';
import { castleKinds, castleNames, castleCommands, castleTechs, castleTraining, castleUnitNames } from '/interface/castle.js';
import { icon, entityIcon, setEntityIcon, techIds, trainingKinds } from '/interface/ui-art.js';
// DOM presentation for authoritative Rust building state and footprint previews.
const byId = id => document.getElementById(id);
export const buildingKinds = ['TownCenter','House','LumberCamp','MiningCamp','Mill','Farm','Barracks','PalisadeWall','Outpost','Dock','ArcheryRange','Stable','Blacksmith','Market','WatchTower','PalisadeGate',...castleKinds,'Bridge','StoneWall','StoneGate'];
export const buildingNames = ['Town Center','House','Lumber Camp','Mining Camp','Mill','Farm','Barracks','Palisade Wall','Outpost','Dock','Archery Range','Stable','Blacksmith','Market','Watch Tower','Palisade Gate',...castleNames,'Bridge','Stone Wall','Stone Gate'];
// Codes are emitted by Rust building_ui::availability, where the rules live.
const disabledReasons=['Unavailable','','requires Feudal Age','Already researched','Research in progress','Wait for the training queue to finish','Not enough food','Not enough wood','Not enough gold','Not enough stone','Training queue is full','Building is under construction','Requires prerequisite buildings or technology','Select your own building','requires Castle Age','A Vassal already belongs to this Castle'];
const action=(button,label,code,observer)=>{const reason=observer?'Observer mode':disabledReasons[code]??disabledReasons[0];button.disabled=code!==1||observer;button.title=label+(reason?` · ${reason}`:'');button.setAttribute('aria-label',button.title);};
const commands = [
  ['house','House','Q','25 wood'], ['lumber-camp','Lumber Camp','R','100 wood'],
  ['mining-camp','Mining Camp','E','100 wood'], ['mill','Mill','W','100 wood'],
  ['farm','Farm','A','60 wood · needs Mill'], ['barracks','Barracks','Q','175 wood'],
  ['palisade-wall','Palisade Wall','S','2 wood / tile'], ['outpost','Outpost','A','25 wood · 5 stone'],
  ['dock','Dock','T','150 wood · needs a riverbank or coast'],
  ['archery-range','Archery Range','W','175 wood · needs Barracks'], ['stable','Stable','E','175 wood · needs Barracks'],
  ['blacksmith','Blacksmith','S','150 wood'], ['market','Market','D','175 wood · needs Mill'],
  ['watch-tower','Watch Tower','F','35 wood · 125 stone'], ['palisade-gate','Palisade Gate','X','30 wood'], ...castleCommands,
  ['bridge','Bridge','G','25 wood · 5 stone / water tile'],
  ['stone-wall','Stone Wall','Z','5 stone / tile'], ['stone-gate','Stone Gate','G','30 stone'],
  ['town-center','Town Center','C','275 wood · 100 stone'],
];
export function initBuildings() {
  initGarrison();
  const grid = byId('command-grid');
  const autoScout=document.createElement('button');autoScout.id='auto-scout';autoScout.hidden=true;autoScout.innerHTML=entityIcon('Scout')+'<span class="command-label">Auto-scout</span>';autoScout.title='Auto-scout · explore the nearest reachable frontier';autoScout.setAttribute('aria-pressed','false');grid.append(autoScout);
  const formationDots=[
    [[5,7],[12,7],[19,7],[26,7],[5,16],[12,16],[19,16],[26,16]],
    [[6,3],[16,3],[26,3],[6,12],[26,12],[6,21],[16,21],[26,21]],
    [[5,5],[16,5],[27,5],[10,17],[21,17]],
    [[3,6],[10,6],[22,6],[29,6],[3,18],[10,18],[22,18],[29,18]],
  ];
  const formations=['Line','Box','Staggered','Flank'].map((name,i)=>{const button=document.createElement('button');button.id=`formation-${name.toLowerCase()}`;button.className='formation-command';button.hidden=true;button.type='button';button.innerHTML=`<svg class="formation-icon" viewBox="0 0 32 24" aria-hidden="true">${formationDots[i].map(([x,y])=>`<circle cx="${x}" cy="${y}" r="2.2"/>`).join('')}</svg><span class="formation-name">${name}</span>`;button.title=`${name} formation`;button.setAttribute('aria-label',button.title);button.setAttribute('aria-pressed',String(i===0));button.addEventListener('click',()=>formations.forEach(b=>b.setAttribute('aria-pressed',String(b===button))));grid.append(button);return button;});
  const attackMove=document.createElement('button');attackMove.id='attack-move-units';attackMove.hidden=true;attackMove.type='button';attackMove.innerHTML='<svg class="formation-icon" viewBox="0 0 32 24" aria-hidden="true"><path d="M7 20 24 3M13 3h11v11" fill="none" stroke="currentColor" stroke-width="3"/></svg><span class="formation-name">Attack move</span>';attackMove.title='Advance in formation and engage visible enemies';grid.append(attackMove);
  const reseedButtons=['add','remove'].map(side=>{const button=document.createElement('button');button.id=`${side}-farm-reseed`;button.hidden=true;button.innerHTML=entityIcon('Farm')+`<span class="command-label">${side==='add'?'Queue farm reseed':'Remove farm reseed'}</span>`;grid.append(button);return button;});
  const trainIcon=document.createElement('span');trainIcon.className='train-icon';
  byId('train-villager').querySelector(':scope>.icon').replaceWith(trainIcon);
  setEntityIcon(trainIcon,'Villager');
  const trainingName=document.createElement('span');trainingName.className='training-unit';trainingName.hidden=true;byId('train-villager').append(trainingName);
  const build = document.createElement('button');
  build.id='build-menu'; build.hidden=true; build.innerHTML=icon('commands','build','towncenter')+'<kbd>B</kbd><small>Economic</small>'; build.title='Economic buildings (B)'; grid.append(build);
  const military=document.createElement('button');military.id='build-military-menu';military.hidden=true;military.innerHTML=icon('commands','military','soldier')+'<kbd>V</kbd><small>Military</small>';military.title='Military buildings (V)';grid.append(military);
  const militaryKinds=new Set([5,6,7,9,10,13,14,15,18,20,21]);
  const category=i=>militaryKinds.has(i)?'military':'economy';
  const buttons=commands.map(([id,name,key,cost],i)=>{
    const button=document.createElement('button'); button.id=`build-${id}`; button.className="build-command"; button.hidden=true;
    button.title=`${name} · ${cost} · ${key}`; button.setAttribute('aria-label',button.title);
    button.innerHTML=`${entityIcon(buildingKinds[(i+1)%buildingKinds.length])}<span class="command-label">${name}</span><kbd>${key}</kbd><small>${cost}</small>`; grid.append(button); return button;
  });
  const research=['Loom · 50 gold','Town Watch · 75 food','Feudal Age · 500 food','Double-Bit Axe · 100 food · 50 wood','Horse Collar · 75 food · 75 wood','Gold Mining · 100 food · 75 wood','Stone Mining · 100 food · 75 wood','Wheelbarrow · 175 food · 50 wood','Man-at-Arms · 100 food · 40 gold','Forging · 150 food','Scale Mail Armor · 100 food','Scale Barding Armor · 150 food','Fletching · 100 food · 50 gold','Padded Archer Armor · 100 food',...castleTechs.map(t=>t[1]),'War Galley · 230 food · 100 gold'];
  const sites=[0,0,0,2,4,3,3,0,6,12,12,12,12,12,...castleTechs.map(t=>t[2]),9];
  const buildStart=8+research.length, trainKind=buildStart+commands.length, trainEnabled=trainKind+1, foundationCount=trainKind+2;
  const techs=['loom','town-watch','feudal-age','double-bit-axe','horse-collar','gold-mining','stone-mining','wheelbarrow','man-at-arms','forging','scale-mail-armor','scale-barding-armor','fletching','padded-archer-armor',...castleTechs.map(t=>t[0]),'war-galley'].map((id,i)=>{
    const button=document.createElement('button'); button.id=`research-${id}`;button.className="research-command";button.hidden=true;button.innerHTML=icon('technologies',techIds[i],'technology')+`<span class="command-label">${i===8?'Research Man-at-Arms':research[i]}</span>`;grid.append(button);return button;
  });
  const militaryTrain=[['archer','Archer','25 wood · 45 gold',10,4],['skirmisher','Skirmisher','25 food · 35 wood',10,5],['spearman','Spearman','35 food · 25 wood',6,6],['scout','Scout Cavalry','80 food',11,1],...castleTraining.filter(t=>t[0]!=='long-swordsman')].map(([id,name,cost,site,kind])=>{
    const button=document.createElement('button');button.id=`train-${id}`;button.hidden=true;button.innerHTML=entityIcon(trainingKinds[kind])+`<span class="command-label">${name} · ${cost}</span>`;button.title=`Train ${name} · ${cost}`;button.setAttribute('aria-label',button.title);grid.append(button);return {button,site,kind};
  });
  const navalTrain=[['galley','Galley',17,'90 wood · 30 gold'],['fire-ship','Fire Ship',18,'75 wood · 45 gold'],['demolition-ship','Demolition Ship',19,'70 wood · 50 gold'],['transport-ship','Transport Ship',20,'125 wood'],['war-galley','War Galley',21,'90 wood · 30 gold']].map(([id,name,kind,cost])=>{const button=document.createElement('button');button.id=`train-${id}`;button.hidden=true;button.innerHTML=entityIcon(trainingKinds[kind])+`<span class="command-label">${name}</span><small>${cost}</small>`;grid.append(button);return {button,kind,name,cost};});
  const unload=document.createElement('button');unload.id='unload-transport';unload.hidden=true;unload.innerHTML=entityIcon('TransportShip')+'<span class="command-label">Unload passengers</span>';grid.append(unload);
  const trades=['wood','food','stone'];
  const tradeButtons=['buy','sell'].flatMap(side=>trades.map(res=>{const button=document.createElement('button');button.id=`market-${side}-${res}`;button.className='build-command market-command';button.hidden=true;grid.append(button);return {button,side,res};}));
  // A line keeps its grid cell even when a different technology disappears.
  const roots=new Map();
  for(const [from,to] of [
    ['train-archer','train-crossbowman'],['train-skirmisher','train-elite-skirmisher'],
    ['train-spearman','train-pikeman'],['train-scout','train-light-cavalry'],['train-galley','train-war-galley'],
    ...[['town-watch','town-patrol'],['feudal-age','castle-age'],['double-bit-axe','bow-saw'],
      ['horse-collar','heavy-plow'],['gold-mining','gold-shaft-mining'],['wheelbarrow','hand-cart'],
      ['man-at-arms','long-swordsman'],['forging','iron-casting'],['scale-mail-armor','chain-mail-armor'],
      ['scale-barding-armor','chain-barding-armor'],['fletching','bodkin-arrow'],
      ['padded-archer-armor','leather-archer-armor']].map(([from,to])=>[`research-${from}`,`research-${to}`]),
  ])roots.set(to,roots.get(from)??from);
  const slots=(entries,row,reserved=new Map())=>{
    const groups=new Map();
    for(const {button,site} of entries){
      if(!groups.has(site))groups.set(site,new Map());
      const group=groups.get(site),root=roots.get(button.id)??button.id;
      if(!group.has(root))group.set(root,group.size+(reserved.get(site)??0));
      const index=group.get(root);
      button.style.gridRow=String(row(site)+Math.floor(index/5));
      button.style.gridColumn=String(index%5+1);
    }
  };
  slots([{button:byId('train-villager'),site:0}],()=>1);
  slots([...militaryTrain,...navalTrain.map(entry=>({...entry,site:9}))],()=>1,new Map([[6,1],[9,1]]));
  slots(techs.map((button,i)=>({button,site:sites[i]})),site=>[0,6,9,10,11].includes(site)?2:1);
  const panel=document.createElement('div');panel.id='building-panel';panel.hidden=true;
  panel.innerHTML='<b id="building-hp"></b><div id="building-status"></div><progress id="construction-progress" max="1000"></progress><div id="research-status"><span id="research-icon"></span><span id="research-label"></span><progress id="research-progress" max="100" aria-label="Research progress" hidden></progress></div><div id="market-prices"></div><button id="cancel-construction" hidden>Cancel construction</button><button id="cancel-research" hidden>Cancel research</button>';
  byId('selection-content').insertBefore(panel,byId('training'));
  const reseedQueue=document.createElement('div');reseedQueue.id='farm-reseed-queue';reseedQueue.hidden=true;reseedQueue.innerHTML='Farm reseeds queued: <span id="farm-reseed-count">0</span>';panel.append(reseedQueue);
  const progressLabels=document.createElement('div');progressLabels.id='construction-labels';document.body.append(progressLabels);
  let open=false, state=[], lastPreview='', queue=[], previousQueue='', previousResearch=-1, previousLabels='';
  const menu=()=>{
    const villager=byId('selection').dataset.villagers==='true';
    build.hidden=!villager;military.hidden=!villager;if(!villager)open=false;
    buttons.forEach((b,i)=>{b.hidden=!villager||open!==category(i);b.disabled=state[buildStart+i]!==1;const [id,name,key,cost]=commands[i];b.title=`${name} · ${cost} · ${category(i)==='military'?'V':'B'} ${key}${(i>=15&&i<=18||id==='town-center')&&state[0]<2?' · requires Castle Age':state[0]===0&&(i>=9&&i<=13||i>=19&&i<=21)?' · requires Feudal Age':''}`;b.setAttribute('aria-label',b.title);});
    if(open){for(const id of ['move-units','stop-units'])byId(id).hidden=true;byId('command-context').textContent=open==='military'?'Military buildings':'Economic buildings';byId('command-hint').textContent=byId('scene').dataset.orderMode==='build'?(byId('scene').title||'Left-click a green footprint · drag walls or bridges bank to bank · Esc cancels'):'Choose a building · Esc closes the menu';}
    grid.classList.toggle('building-menu-open',Boolean(open));
    build.setAttribute('aria-expanded',String(open==='economy'));military.setAttribute('aria-expanded',String(open==='military'));
    const active=Array.from(grid.children).filter(n=>n.tagName==='BUTTON'&&!n.hidden).length;
    grid.querySelectorAll('.empty-command').forEach((n,i)=>{n.hidden=i>=Math.max(0,15-active);});
  };
  build.addEventListener('click',()=>{open=open==='economy'?false:'economy';menu();});
  military.addEventListener('click',()=>{open=open==='military'?false:'military';menu();});
  window.aoeBuildings={
    bind(callback) {
      [...buttons,...techs,byId('cancel-construction'),byId('cancel-research'),...reseedButtons,autoScout].forEach((button,i)=>button.addEventListener('click',()=>callback(0,i)));
      militaryTrain.forEach(({button,kind})=>button.addEventListener('click',()=>callback(1,kind)));
      tradeButtons.forEach(({button},i)=>button.addEventListener('click',()=>callback(2,i)));
      ['train-villager','stop-units','move-units','cancel-training'].forEach((id,i)=>byId(id).addEventListener('click',()=>callback(3,i)));
      navalTrain.forEach(({button,kind})=>button.addEventListener('click',()=>callback(1,kind)));
      unload.addEventListener('click',()=>callback(4,0));
      formations.forEach((button,i)=>button.addEventListener('click',()=>callback(5,i)));
      attackMove.addEventListener('click',()=>callback(6,0));
      byId('combat-stance').addEventListener('change',event=>{ if(event.target.value!=='') callback(7,Number(event.target.value)); event.target.value=''; });
    },
    queue(values,visible) { queue=values; byId('training').hidden=!visible||!queue.length; },
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
      byId('age').textContent=['I  Dark Age','II  Feudal Age','III  Castle Age'][v[0]]??'I  Dark Age';
      const kind=v[1],selected=kind>=0,owned=byId('selection').dataset.owned==='true';
      const availability=foundationCount+1+(v[foundationCount]??0)*3;
      militaryTrain.forEach(({button,site,kind:unit})=>{button.hidden=!owned||kind!==site||v[4]<1000||v[availability+unit]===0;action(button,`Train ${button.textContent}`,v[availability+unit],!byId('observer-label').hidden);});
      panel.hidden=!selected;
      navalTrain.forEach(({button,kind:unit,name,cost})=>{button.hidden=!owned||kind!==9||v[4]<1000||v[availability+unit]===0;action(button,`Train ${name} · ${cost}`,v[availability+unit],!byId('observer-label').hidden);});
      const market=availability+trainingKinds.length+1;
      const reseed=market+15;
      const transport=v[reseed+6]??0,cargo=v[reseed+7]??0;unload.hidden=!transport;unload.disabled=cargo===0||!byId('observer-label').hidden;unload.title='Right-click a shore to sail there and unload';if(transport)byId('selection-load').textContent=`${cargo}/5 passengers`;transportLoads(v.slice(reseed+8));
      const cavalry=v[reseed+4]??0,scouting=v[reseed+5]??0;autoScout.hidden=cavalry===0;autoScout.disabled=!byId('observer-label').hidden;autoScout.setAttribute('aria-pressed',scouting===0?'false':scouting===cavalry?'true':'mixed');autoScout.querySelector('.command-label').textContent=scouting===cavalry&&cavalry>0?'Stop auto-scout':'Auto-scout';
      reseedQueue.hidden=kind!==4||!owned;byId('farm-reseed-count').textContent=String(v[reseed]??0);
      reseedButtons.forEach((button,i)=>{button.hidden=!owned||kind!==4||v[4]<1000;button.disabled=v[reseed+1+i]!==1||!byId('observer-label').hidden;button.title=i===0?`Queue farm reseed · ${v[reseed+3]} wood`:`Remove farm reseed · refund ${v[reseed+3]} wood`;button.setAttribute('aria-label',button.title);});
      tradeButtons.forEach(({button,side,res},i)=>{const price=v[market+i%3];const gold=v[market+(side==='buy'?9:12)+i%3];button.hidden=!owned||kind!==13||v[4]<1000;button.disabled=v[market+3+i]!==1||!byId('observer-label').hidden;const name=res[0].toUpperCase()+res.slice(1);const signature=`${side}:${res}:${gold}`;if(button.dataset.signature!==signature){button.innerHTML=icon('resources',res,res)+`<span class="command-label">${side==='buy'?'Buy':'Sell'} ${name}</span><span class="trade-side">${side==='buy'?'+':'−'}</span><small>${gold} gold</small>`;button.dataset.signature=signature;}button.dataset.price=String(price);button.title=`${side==='buy'?'Buy':'Sell'} 100 ${res} for ${gold} gold (price ${price})`;button.setAttribute('aria-label',button.title);});
      byId('market-prices')&&(byId('market-prices').textContent=kind===13&&v[4]>=1000?`Prices per 100: wood ${v[market]} · food ${v[market+1]} · stone ${v[market+2]} gold`:'');
      techs.forEach((b,i)=>{b.hidden=!owned||kind!==sites[i]||v[8+i]===0||v[8+i]===3;action(b,`Research ${research[i]}${i===8?' · Upgrades existing and future Militia':''}`,v[8+i],!byId('observer-label').hidden);});
      const train=byId('train-villager');
      const unitNames=['Villager','Scout Cavalry','Militia','Fishing Ship','Archer','Skirmisher','Spearman','Man-at-Arms',...castleUnitNames,'Galley','Fire Ship','Demolition Ship','Transport Ship','War Galley','Vassal','General'];
      const infantry=unitNames[v[availability+trainingKinds.length]]??'Militia';
      if(selected){train.hidden=!owned||![0,6,9].includes(kind)||v[4]<1000;action(train,kind===9?'Train Fishing Ship, 75 wood, Q':kind===6?`Train ${infantry}, 50 food, 20 gold, Q`:'Train Villager, 50 food, Q',v[trainEnabled],!byId('observer-label').hidden);trainingName.hidden=kind!==6;trainingName.textContent=infantry;setEntityIcon(trainIcon,kind===9?'FishingShip':kind===6?trainingKinds[v[availability+trainingKinds.length]]:'Villager');train.querySelector('.cost').textContent=kind===9?'75 W':kind===6?'50 F · 20 G':'50';}
      const labels=Array.from({length:v[foundationCount]??0},(_,i)=>{const at=foundationCount+1+i*3;return `<span style="left:${v[at]}px;top:${v[at+1]}px">${Math.floor(v[at+2]/10)}%<progress max="1000" value="${v[at+2]}"></progress></span>`;}).join('');
      if(labels!==previousLabels){progressLabels.innerHTML=labels;previousLabels=labels;}
      if(selected){if(owned)byId('command-hint').textContent=v[4]<1000?'Right-click this foundation with villagers to resume construction':kind===0?'Q: train a villager · research below':kind===9?'Q: train a Fishing Ship · 75 wood':kind===6?`Q: train ${infantry} · 50 food, 20 gold`:kind===16?'Train a General to lead your army · train a Vassal to manage the economy':kind===5?'Right-click this Farm with a villager to work it':[10,11].includes(kind)?'Train military units below':kind===12?'Research equipment for your army':'Select villagers to gather or build';byId('selection-task').textContent=v[4]<1000?'Under construction':v[6]>=0?'Researching':byId('training').hidden===false?`Training ${unitNames[v[trainKind]]}`:'Ready';byId('selection-load').textContent=['Resource drop-off · supports 5 population','Supports 5 population','Wood drop-off','Gold and stone drop-off','Food drop-off','Food production','Trains infantry','Blocks movement','Scouts the surrounding area','Fish drop-off · trains fishing, transport and war ships','Trains Archers and Skirmishers','Trains Scout Cavalry','Upgrades infantry, cavalry and archers','Market · buy and sell wood, food and stone for gold','Arrow defence · 5 pierce attack · range 1–8','Opens for nearby friendly units','Arrow defence · 11 pierce attack · five arrows','Trains Monks · nearby injured units heal automatically','Research building and ranged upgrades','Trains Mangonels and Battering Rams','Connects land across water · crossing along its length only'][kind];}
      byId('building-hp').textContent=`${v[2]} / ${v[3]} HP`;
      byId('building-status').textContent=v[4]<1000?`Constructing · ${Math.floor(v[4]/10)}%`:v[5]>=0?`Complete · ${v[5]} food remaining`:'Complete';
      byId('construction-progress').hidden=v[4]>=1000;byId('construction-progress').value=v[4];
      byId('cancel-construction').hidden=!owned||!selected||v[4]>=1000;
      byId('cancel-research').hidden=!owned||v[6]<0;
      const researching=v[6]>=0;
      if(v[6]!==previousResearch){byId('research-icon').innerHTML=researching?icon('technologies',techIds[v[6]],'technology'):'';previousResearch=v[6];}
      byId('research-label').textContent=researching?`${research[v[6]].split(' · ')[0]} · ${Math.floor(v[7]*100)}%`:'';
      byId('research-progress').hidden=!researching;byId('research-progress').value=Math.floor(v[7]*100);
      if(queue.length){
        const percent=Math.floor(queue[0][0]/Math.max(1,queue[0][1])*100), name=unitNames[queue[0][2]];
        byId('training-progress').value=percent;byId('training-progress').setAttribute('aria-label',`${name} training progress`);
        const waiting=v[reseed+8]===1;
        byId('training-label').textContent=waiting?`Waiting for housing · ${name} · ${percent}%`:`Training ${name} · ${percent}%`;
        if(waiting)byId('selection-task').textContent='Waiting for housing';
        byId('cancel-training').title=`Cancel active ${name} training and refund its cost`;
        const signature=queue.map(entry=>entry[2]).join(',');
        if(signature!==previousQueue){byId('training-queue').innerHTML=queue.map(([,,kind])=>`<span class="unit-token" title="${unitNames[kind]} queued">${entityIcon(trainingKinds[kind])}</span>`).join('');previousQueue=signature;}
      } else previousQueue='';
      menu();
    },
    preview(v) {
      const signature=Array.from(v).join(','); if(signature===lastPreview)return; lastPreview=signature;
      byId('scene').dataset.placementValid=String(v.length>0&&v[0]===1);
      byId('scene').dataset.placementCount=String(v[1]??0);
      byId('scene').dataset.placementInvalid=String(v[2]??0);
      const reasons=['','Drag between two land banks','Bridge is too long or has an invalid direction','Bridge start must be on land','Bridge end must be on land','Not enough resources for this bridge','Bridge tiles must be clear and cross water'];
      byId('scene').title=reasons[v[3]??0]??'';
      if(open&&byId('scene').dataset.orderMode==='build')byId('command-hint').textContent=byId('scene').title||'Left-click a green footprint · drag walls or bridges bank to bank · Esc cancels';
    },
  };
}
