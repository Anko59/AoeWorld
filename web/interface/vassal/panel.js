// Geometry and ownership arrive from Rust, using the normal camera projection.
export function initVassal() {
  const ns='http://www.w3.org/2000/svg';
  const layer=document.createElementNS(ns,'svg');layer.id='vassal-domain';
  layer.setAttribute('aria-label','Vassal domain, 100 tile radius');
  layer.style.cssText='position:fixed;inset:0;width:100%;height:100%;pointer-events:none;z-index:2;overflow:hidden;display:none';
  const ring=document.createElementNS(ns,'polygon');
  ring.setAttribute('fill','rgba(234,194,77,.035)');ring.setAttribute('stroke','#eac24d');
  ring.setAttribute('stroke-width','2');ring.setAttribute('vector-effect','non-scaling-stroke');
  layer.append(ring);document.body.append(layer);
  const status=document.createElement('div');status.id='vassal-status';status.hidden=true;
  document.getElementById('selection-content').append(status);
  let previous='';
  window.aoeVassal={update(values,label){
    const visible=values.length>2;layer.style.display=visible?'block':'none';
    if(visible){
      layer.dataset.active=String(values[0]===1);layer.dataset.radius=String(values[1]);
      ring.setAttribute('stroke-dasharray',values[0]===1?'':'8 6');
      const points=[];for(let i=2;i+1<values.length;i+=2)points.push(`${values[i]},${values[i+1]}`);
      const next=points.join(' ');if(next!==previous){ring.setAttribute('points',next);previous=next;}
    }
    status.hidden=!label;if(status.textContent!==label)status.textContent=label;
  }};
}
