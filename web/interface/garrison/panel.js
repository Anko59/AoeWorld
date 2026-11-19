import { icon, entityIcon, trainingKinds } from '/interface/ui-art.js';
import { autoFit } from '/interface/selection-grid.js';

export function initGarrison() {
  const grid=document.getElementById('command-grid');
  const enter=document.createElement('button');enter.id='garrison';enter.hidden=true;
  enter.innerHTML=icon('commands','garrison','towncenter')+'<span class="command-label">Garrison</span>';
  enter.title='Garrison · click a friendly Town Center, Castle or tower';grid.append(enter);
  const eject=document.createElement('button');eject.id='ungarrison';eject.hidden=true;
  eject.innerHTML=icon('commands','ungarrison','move')+'<span class="command-label">Ungarrison all</span>';
  eject.title='Ungarrison all units onto free nearby ground';grid.append(eject);
  const controls = [
    ['back-to-work', 'Back to Work', 'Release villagers to their previous jobs', 'villager'],
    ['town-bell', 'Town Bell', 'Ring for shelter · ring again to send villagers back to work', 'towncenter'],
  ].map(([id, label, hint, fallback]) => {
    const button = document.createElement('button');
    button.id=id;button.type='button';button.hidden=true;
    button.innerHTML=icon('commands',id,fallback)+`<span class="command-label">${label}</span>`;
    button.setAttribute('aria-label',label);
    button.title=hint;grid.append(button);
    return button;
  });
  const panel=document.createElement('div');panel.id='garrison-panel';panel.hidden=true;
  panel.innerHTML='<b>Garrison: <span id="garrison-count"></span></b><div id="garrison-units"></div>';
  document.getElementById('selection-content').append(panel);
  autoFit(document.getElementById('garrison-units'),{high:36,low:28});
  let previous='';
  window.aoeGarrison={update(values){
    const capacity=values[0], count=(values.length-4)/4;
    const [backToWork,bell]=controls;
    backToWork.hidden=capacity===0;
    backToWork.disabled=values[3]!==1;
    bell.hidden=values[2]!==1;
    enter.hidden=values[1]!==1;enter.disabled=false;
    eject.hidden=capacity===0;eject.disabled=count===0;
    panel.hidden=capacity===0;
    document.getElementById('garrison-count').textContent=`${count} / ${capacity}`;
    const signature=Array.from(values.slice(4)).join(',');
    if(signature===previous){for(const button of panel.querySelectorAll('button'))button.disabled=false;return;}previous=signature;
    const units=document.getElementById('garrison-units');units.replaceChildren();
    for(let at=4;at<values.length;at+=4){
      const button=document.createElement('button');button.dataset.unitId=String(values[at]);
      button.disabled=false;button.innerHTML=entityIcon(trainingKinds[values[at+1]]);
      button.title=`Ungarrison ${trainingKinds[values[at+1]]} · ${values[at+2]} / ${values[at+3]} HP`;
      button.setAttribute('aria-label',button.title);units.append(button);
    }
  }};
}
