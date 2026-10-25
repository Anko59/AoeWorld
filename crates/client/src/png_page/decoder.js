// Byte-exact RGBA8 atlas decoding. Never pass source art through canvas colour conversion.
const SIDE = 2048, MAX_INPUT = 32 * 1024 * 1024, WRITE_CHUNK = 16 * 1024;
const SIGNATURE = [137,80,78,71,13,10,26,10];
const ADAM7 = [[0,0,8,8],[4,0,8,8],[0,4,4,8],[2,0,4,4],[0,2,2,4],[1,0,2,2],[0,1,1,2]];
const SINGLE = [[0,0,1,1]];
const crcTable = Uint32Array.from({length:256}, (_, value) => {
  for (let bit=0;bit<8;bit++) value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
  return value >>> 0;
});
const invalid = message => { throw Error(message); };
// Metadata only: never retain source pixels or compressed pages across decodes.
const validated=new Map(), MAX_VALIDATED=512;
function idatBytes(bytes,total) {
  const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);
  const idat=new Uint8Array(total);
  let at=0;
  for (let offset=8;offset<bytes.length;) {
    const length=view.getUint32(offset);
    if (view.getUint32(offset+4)===0x49444154) {
      idat.set(bytes.subarray(offset+8,offset+8+length),at);at+=length;
    }
    offset+=length+12;
  }
  return idat;
}

export function validate_header(bytes) {
  if (bytes.length > MAX_INPUT) invalid('Atlas PNG exceeds 32 MiB');
  if (bytes.length < 33) invalid('Invalid atlas PNG header');
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (SIGNATURE.some((b,i)=>bytes[i]!==b) || view.getUint32(8)!==13 || view.getUint32(12)!==0x49484452)
    invalid('Invalid atlas PNG header');
  if (view.getUint32(16)!==SIDE || view.getUint32(20)!==SIDE) invalid('Unsupported atlas dimensions');
  if (bytes[24]!==8 || bytes[25]!==6 || bytes[26]!==0 || bytes[27]!==0 || bytes[28]>1)
    invalid('Unsupported atlas pixel format');
}

function parse(bytes) {
  validate_header(bytes);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let offset=8, total=0, sawIdat=false, idatEnded=false, sawEnd=false, sawPalette=false;
  const interlaced=bytes[28]===1;
  while (offset<bytes.length) {
    if (offset+8>bytes.length) invalid('Truncated atlas PNG chunk');
    const length=view.getUint32(offset), end=offset+length+12;
    if (end>bytes.length) invalid('Invalid atlas PNG chunk length');
    const kindBytes=bytes.subarray(offset+4,offset+8);
    if (!kindBytes.every(b=>b>=65&&b<=90||b>=97&&b<=122) || (kindBytes[2]&32)!==0)
      invalid('Invalid atlas PNG chunk type');
    let crc=0xffffffff;
    for (let i=offset+4;i<end-4;i++) crc=crcTable[(crc^bytes[i])&255]^(crc>>>8);
    if (((crc^0xffffffff)>>>0)!==view.getUint32(end-4)) invalid('Atlas PNG checksum mismatch');
    const kind=String.fromCharCode(...kindBytes);
    if (offset===8) {
      if (kind!=='IHDR'||length!==13) invalid('Invalid atlas PNG header');
    } else if (kind==='IHDR') invalid('Duplicate atlas PNG header');
    if (sawIdat&&kind!=='IDAT') idatEnded=true;
    if (kind==='IHDR') { /* validated above */ }
    else if (kind==='PLTE'&&!sawIdat&&!sawPalette&&length>=3&&length<=768&&length%3===0) sawPalette=true;
    else if (kind==='tRNS') invalid('Transparency chunk is invalid for RGBA atlas PNGs');
    else if (kind==='IDAT'&&!idatEnded) {
      sawIdat=true;total+=length;
      if (total>MAX_INPUT) invalid('Atlas PNG compressed data exceeds 32 MiB');
    } else if (kind==='IEND'&&sawIdat&&length===0) {
      sawEnd=true;
      if (end!==bytes.length) invalid('Data follows atlas PNG end');
    } else if ((kindBytes[0]&32)===0) invalid('Unsupported atlas PNG chunk');
    offset=end;
    if (sawEnd) break;
  }
  if (!sawIdat||!sawEnd) invalid('Incomplete atlas PNG');
  // Own compressed input before the first await: the WASM caller releases its source buffer.
  const idat=idatBytes(bytes,total);
  return {idat,interlaced};
}

export async function decode_page(bytes) {
  validate_header(bytes);
  // Own borrowed WASM bytes synchronously, before the digest promise can yield.
  let owned=bytes.slice(), page;
  if (globalThis.crypto?.subtle) {
    const digest=new Uint8Array(await crypto.subtle.digest('SHA-256',owned));
    const key=Array.from(digest,b=>b.toString(16).padStart(2,'0')).join('');
    const prior=validated.get(key);
    if (prior) page={idat:idatBytes(owned,prior.total),interlaced:prior.interlaced};
    else {
      page=parse(owned);
      if (validated.size===MAX_VALIDATED) validated.delete(validated.keys().next().value);
      validated.set(key,{total:page.idat.length,interlaced:page.interlaced});
    }
  } else page=parse(owned);
  owned=null;
  const scanlines=await inflateOwned(page.idat,filtered_size(SIDE,SIDE,page.interlaced));
  return reconstruct(scanlines,SIDE,SIDE,page.interlaced);
}

// The exported test boundary must own borrowed WASM bytes before yielding too.
export function inflate(bytes,limit) { return inflateOwned(bytes.slice(),limit); }
async function inflateOwned(compressed,limit) {
  if (!Number.isInteger(limit)||limit<0||limit>filtered_size(SIDE,SIDE,true)) invalid('Invalid atlas output bound');
  const stream=new DecompressionStream('deflate');
  const reader=stream.readable.getReader(), writer=stream.writable.getWriter();
  const output=new Uint8Array(limit);
  let length=0, pending=reader.read();
  const consume=record=>{
    if (record.done===true) return true;
    const chunk=record.value;
    if (!(chunk instanceof Uint8Array)) invalid('Invalid atlas PNG stream record');
    if (length+chunk.length>limit) invalid('Atlas PNG expands beyond its dimension limit');
    output.set(chunk,length);length+=chunk.length;return false;
  };
  try {
    for (let at=0;at<compressed.length;at+=WRITE_CHUNK) {
      const write=writer.write(compressed.slice(at,at+WRITE_CHUNK));
      for (;;) {
        const record=await Promise.race([pending,write]);
        if (record===undefined) break;
        if (consume(record)) invalid('Truncated atlas PNG compressed stream');
        pending=reader.read();
      }
    }
    const close=writer.close();
    let closeDone=false;
    for (;;) {
      const record=closeDone?await pending:await Promise.race([pending,close]);
      if (record===undefined) { closeDone=true;continue; }
      if (consume(record)) { await close;break; }
      pending=reader.read();
    }
    return output.subarray(0,length);
  } catch (reason) {
    const cancel=reader.cancel(reason), abort=writer.abort(reason);
    await Promise.allSettled([cancel,abort]);
    throw reason;
  }
}

const passSize=(length,start,step)=>length<=start?0:Math.ceil((length-start)/step);
export function filtered_size(width,height,interlaced) {
  if (![width,height].every(n=>Number.isInteger(n)&&n>=0&&n<=SIDE)) invalid('Atlas PNG dimensions overflow');
  let total=0;
  for (const [x,y,sx,sy] of interlaced?ADAM7:SINGLE) {
    const w=passSize(width,x,sx), h=passSize(height,y,sy);
    if (w&&h) total+=(w*4+1)*h;
  }
  return total;
}

export function reconstruct(scanlines,width,height,interlaced) {
  if (scanlines.length!==filtered_size(width,height,interlaced)) invalid('Invalid atlas PNG scanline data');
  const pixels=new Uint8Array(width*height*4);
  let source=0;
  for (const [x,y,sx,sy] of interlaced?ADAM7:SINGLE) {
    const w=passSize(width,x,sx), h=passSize(height,y,sy);
    if (!w||!h) continue;
    const rowBytes=w*4;
    let previous=new Uint8Array(rowBytes), row=new Uint8Array(rowBytes);
    for (let r=0;r<h;r++) {
      const filter=scanlines[source++];
      row.set(scanlines.subarray(source,source+rowBytes));source+=rowBytes;
      unfilter(row,previous,filter);
      const targetY=y+r*sy;
      if (sx===1) pixels.set(row,(targetY*width+x)*4);
      else for (let col=0;col<w;col++) pixels.set(row.subarray(col*4,col*4+4),(targetY*width+x+col*sx)*4);
      [previous,row]=[row,previous];
    }
  }
  return pixels;
}

function unfilter(row,previous,filter) {
  if (filter>4) invalid('Unsupported atlas PNG filter');
  if (filter===0) return;
  if (filter===1) {
    // Imported atlas pages use Sub. Add all four RGBA lanes together without
    // carries crossing byte boundaries, instead of four byte-indexed loops.
    const words=new Uint32Array(row.buffer,row.byteOffset,row.length/4);
    let left=words[0];
    for (let i=1;i<words.length;i++) {
      const value=words[i];
      if (value===0&&left===0) continue; // Transparent atlas padding is already exact.
      const rb=((value&0x00ff00ff)+(left&0x00ff00ff))&0x00ff00ff;
      const ga=(((value>>>8)&0x00ff00ff)+((left>>>8)&0x00ff00ff))&0x00ff00ff;
      words[i]=left=rb|(ga<<8);
    }
    return;
  }
  for (let i=0;i<row.length;i++) {
    const left=i>=4?row[i-4]:0, above=previous[i], corner=i>=4?previous[i-4]:0;
    let predictor;
    if (filter===2) predictor=above;
    else if (filter===3) predictor=Math.floor((left+above)/2);
    else {
      const estimate=left+above-corner;
      const dl=Math.abs(estimate-left), da=Math.abs(estimate-above), dc=Math.abs(estimate-corner);
      predictor=dl<=da&&dl<=dc?left:da<=dc?above:corner;
    }
    row[i]+=predictor;
  }
}
