// Geometry and ownership arrive from Rust, using the normal camera projection.
// values = [count, (active, selected, points, x0, y0, ...) * count]. Territories
// of overlapping castles are split at equal distance; each cell's outline stops
// short of the shared border, so two lines show where the influences meet.
export function initSteward() {
  const ns='http://www.w3.org/2000/svg';
  const layer=document.createElementNS(ns,'svg');layer.id='steward-domain';
  layer.setAttribute('aria-label','Steward domains, 100 tile radius');
  layer.style.cssText='position:fixed;inset:0;width:100%;height:100%;pointer-events:none;z-index:2;overflow:hidden;display:none';
  document.body.append(layer);
  const rings=[];
  const ring=(i)=>{
    if(!rings[i]){
      const p=document.createElementNS(ns,'polygon');
      p.setAttribute('stroke','#eac24d');p.setAttribute('vector-effect','non-scaling-stroke');
      layer.append(p);rings[i]={node:p,points:''};
    }
    return rings[i];
  };
  const status=document.createElement('div');status.id='steward-status';status.hidden=true;
  document.getElementById('selection-content').append(status);
  const controls=document.createElement('div');controls.id='steward-controls';controls.hidden=true;
  const sliders=[];
  for(const [i,name] of ['Food weight','Wood weight','Gold weight','Stone weight','Villager cap'].entries()){
    const row=document.createElement('label');row.textContent=name;
    const input=document.createElement('input');input.type='range';input.min='0';input.max='100';input.step='1';
    input.id=`steward-setting-${i}`;input.setAttribute('aria-label',name);
    const value=document.createElement('output');input.addEventListener('input',()=>{value.textContent=input.value;});
    row.append(input,value);controls.append(row);sliders.push({input,value});
  }
  const toggle=document.createElement('input');toggle.type='checkbox';toggle.id='steward-training';
  const train=document.createElement('label');train.append(toggle,'Train villagers and fishing ships');controls.append(train);
  const dismiss=document.createElement('button');dismiss.id='steward-dismiss';dismiss.textContent='Dismiss idle villagers';
  dismiss.title='Kill idle own villagers in this territory that no Steward manages';controls.append(dismiss);
  document.getElementById('selection-content').append(controls);
  let settingsSignature='',confirmedWeights=[];
  window.aoeSteward={
  bind(callback){controls.addEventListener('change',()=>{
    // Keep the last server-confirmed mix when the final positive weight is
    // dragged to zero. Never submit a mix that would poison later changes.
    if(sliders.slice(0,4).every(s=>Number(s.input.value)===0)){
      confirmedWeights.forEach((weight,i)=>{
        sliders[i].input.value=String(weight);sliders[i].value.textContent=String(weight);
      });
      return;
    }
    callback(0);
  });dismiss.addEventListener('click',()=>callback(1));},
  settings(){return new Uint8Array([...sliders.map(s=>Number(s.input.value)),Number(toggle.checked)]);},
  update(values,label,settings){
    controls.hidden=settings.length===0;
    const signature=Array.from(settings).join(',');
    if(signature!==settingsSignature){
      settingsSignature=signature;
      if(settings.length===7){
        confirmedWeights=Array.from(settings.slice(1,5));
        for(let i=0;i<5;i++){sliders[i].input.value=String(settings[i+1]);sliders[i].value.textContent=String(settings[i+1]);}
        toggle.checked=settings[6]===1;
      }
    }
    const count=values.length>0?values[0]:0;layer.style.display=count>0?'block':'none';
    layer.dataset.count=String(count);layer.dataset.radius='100';
    let at=1;
    for(let i=0;i<count;i++){
      const active=values[at]===1,selected=values[at+1]===1,n=values[at+2];at+=3;
      const r=ring(i);
      r.node.style.display='';
      r.node.setAttribute('fill',active?'rgba(234,194,77,.035)':'none');
      r.node.setAttribute('stroke-width',selected?'2.5':'1.5');
      r.node.setAttribute('stroke-dasharray',active?'':'8 6');
      r.node.dataset.active=String(active);r.node.dataset.selected=String(selected);
      if(selected)layer.dataset.active=String(active);
      const points=[];for(let k=0;k<n;k++)points.push(`${values[at+2*k]},${values[at+2*k+1]}`);
      at+=2*n;
      const next=points.join(' ');if(next!==r.points){r.node.setAttribute('points',next);r.points=next;}
    }
    for(let i=count;i<rings.length;i++)rings[i].node.style.display='none';
    status.hidden=!label;if(status.textContent!==label)status.textContent=label;
  }};
}
