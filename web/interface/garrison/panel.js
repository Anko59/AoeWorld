import { icon, entityIcon, trainingKinds } from '/interface/ui-art.js';

export function initGarrison() {
  const grid=document.getElementById('command-grid');
  const enter=document.createElement('button');enter.id='garrison';enter.hidden=true;
  enter.innerHTML=icon('commands','garrison','towncenter')+'<span class="command-label">Garrison</span>';
  enter.title='Garrison · click a friendly Town Center, Castle or tower';grid.append(enter);
  const eject=document.createElement('button');eject.id='ungarrison';eject.hidden=true;
  eject.innerHTML=icon('commands','ungarrison','move')+'<span class="command-label">Ungarrison all</span>';
  eject.title='Ungarrison all units onto free nearby ground';grid.append(eject);
  const panel=document.createElement('div');panel.id='garrison-panel';panel.hidden=true;
  panel.innerHTML='<b>Garrison: <span id="garrison-count"></span></b><div id="garrison-units"></div>';
  document.getElementById('selection-content').append(panel);
  let previous='';
  window.aoeGarrison={update(values){
    const capacity=values[0], count=(values.length-2)/4;
    const observer=!document.getElementById('observer-label').hidden;
    enter.hidden=values[1]!==1;enter.disabled=observer;
    eject.hidden=capacity===0;eject.disabled=count===0||observer;
    panel.hidden=capacity===0;
    document.getElementById('garrison-count').textContent=`${count} / ${capacity}`;
    const signature=Array.from(values.slice(2)).join(',');
    if(signature===previous){for(const button of panel.querySelectorAll('button'))button.disabled=observer;return;}previous=signature;
    const units=document.getElementById('garrison-units');units.replaceChildren();
    for(let at=2;at<values.length;at+=4){
      const button=document.createElement('button');button.dataset.unitId=String(values[at]);
      button.disabled=observer;button.innerHTML=entityIcon(trainingKinds[values[at+1]]);
      button.title=`Ungarrison ${trainingKinds[values[at+1]]} · ${values[at+2]} / ${values[at+3]} HP`;
      button.setAttribute('aria-label',button.title);units.append(button);
    }
  }};
}
