// Owner-visible server report. The button never makes local planning decisions.
export function initVassal() {
  const panel=document.createElement('div');panel.id='vassal-panel';panel.hidden=true;
  const status=document.createElement('p');status.id='vassal-status';status.setAttribute('role','status');
  const toggle=document.createElement('button');toggle.id='vassal-development';toggle.type='button';
  panel.append(status,toggle);document.getElementById('selection-content').append(panel);
  window.aoeVassal={
    bind(callback){toggle.addEventListener('click',callback);},
    update(values,label){
      panel.hidden=values.length===0;panel.dataset.alert=String(values[2]===1);status.textContent=label;
      toggle.textContent=values[1]===1?'Stop development':'Resume development';
    },
  };
}
