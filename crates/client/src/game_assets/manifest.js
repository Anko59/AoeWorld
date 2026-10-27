// Startup metadata belongs in the browser's JSON parser, outside the renderer.
// Preserve required types, numeric bounds and duplicate-field rejection.
const string = value => {
  if (typeof value !== 'string' || /[\uD800-\uDFFF]/u.test(value)) throw Error('invalid manifest string');
  return value;
};
const integer = (value, min, max) => {
  if (!Number.isInteger(value) || value < min || value > max) throw Error('invalid manifest integer');
  return value;
};
const u16 = value => integer(value, 0, 65535);
const u32 = value => integer(value, 0, 4294967295);
const i32 = value => integer(value, -2147483648, 2147483647);
const array = value => {
  if (!Array.isArray(value)) throw Error('invalid manifest array');
  return value;
};

// JSON.parse checks grammar. A separate bounded-depth token walk preserves
// serde's rejection of duplicate fields, including escaped equivalent keys.
function uniqueKeys(text) {
  const tokens = text.match(/"(?:\\[\s\S]|[^"\\])*"|[{}\[\]:,]|[^\s{}\[\]:,]+/g) ?? [];
  let at = 0;
  function value(depth) {
    if (depth > 128) throw Error('manifest nesting limit');
    const token = tokens[at++];
    if (token === '{') {
      const keys = new Set();
      if (tokens[at] === '}') { at++; return; }
      for (;;) {
        const key = string(JSON.parse(tokens[at++]));
        if (keys.has(key)) throw Error('duplicate manifest field');
        keys.add(key);
        at++; // colon; JSON.parse already checked grammar
        value(depth + 1);
        if (tokens[at++] === '}') return;
      }
    }
    if (token === '[') {
      if (tokens[at] === ']') { at++; return; }
      for (;;) {
        value(depth + 1);
        if (tokens[at++] === ']') return;
      }
    }
  }
  value(0);
}

export function parse_manifest(bytes) {
  const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  const data = JSON.parse(text);
  uniqueKeys(text);
  string(data.converter);
  string(data.input_hash);
  const pages = array(data.pages).map(page => {
    for (const field of ['color_hash','player_hash','shadow_hash','outline','outline_hash']) string(page[field]);
    return [string(page.color),string(page.player),string(page.shadow),u16(page.width),u16(page.height)];
  });
  const frames = array(data.frames).map(frame => {
    string(frame.source_hash);
    return [string(frame.source),u32(frame.frame),u16(frame.page),u16(frame.x),u16(frame.y),
      u16(frame.width),u16(frame.height),i32(frame.anchor_x),i32(frame.anchor_y)];
  });
  return [u16(data.version),string(data.building_data),pages,frames];
}

export function parse_content(bytes) {
  if (bytes.length > 32768) throw Error('game content exceeds startup budget');
  const data = document(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  const rules = array(data.rules).map(i32);
  const actors = array(data.actors);
  if (actors.length !== 130) throw Error('invalid actor content count');
  const words = actors.flatMap(actor => {
    const row = array(actor);
    if (row.length !== 4) throw Error('invalid actor content record');
    return [u32(row[0]), integer(row[1],1,255), integer(row[2],1,65535), integer(row[3],0,255)];
  });
  return [new Int32Array(rules), new Uint32Array(words)];
}

function document(text) {
  const data = JSON.parse(text);
  uniqueKeys(text);
  return data;
}
const chunk = data => [i32(data.x),i32(data.y),string(data.payload_hex)];
export function parse_chunk(text) { return chunk(document(text)); }
export function parse_chunks(text) { return array(document(text)).map(chunk); }
export function parse_bounds(text, hash) {
  const data = document(text);
  const min = integer(data.minimum_height_level, -32768, 32767);
  const max = integer(data.maximum_height_level, -32768, 32767);
  if (string(data.content_hash) !== hash || min > max) throw Error('invalid map height bounds');
  return [min,max];
}
