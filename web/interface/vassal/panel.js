// Geometry and ownership arrive from Rust, using the normal camera projection.
// values = [count, (active, selected, points, x0, y0, ...) * count]. Territories
// of overlapping castles are split at equal distance; each cell's outline stops
// short of the shared border, so two lines show where the influences meet.
export function initVassal() {
  const ns='http://www.w3.org/2000/svg';
  const layer=document.createElementNS(ns,'svg');layer.id='vassal-domain';
  layer.setAttribute('aria-label','Vassal domains, 100 tile radius');
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
  const status=document.createElement('div');status.id='vassal-status';status.hidden=true;
  document.getElementById('selection-content').append(status);
  window.aoeVassal={update(values,label){
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
