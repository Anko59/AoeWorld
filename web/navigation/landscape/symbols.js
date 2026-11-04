// Architectural map ink, baked once into six tiny surfaces. These are code
// shapes, not copies of the original-art atlases or hundreds of SVG images.
export function settlementSymbols() {
  const paths = {
    village: [['#cbb98a','M9 22h30v21H9z'],['#997246','M3 23 24 5l21 18z'],['#514e3d','M19 43V30q5-6 10 0v13z'],['#d8c594','M21 8V2l11 3-11 3']],
    castle: [['#cdbd92','M5 42V15h5v5h5v-5h5v10h8V15h5v5h5v-5h5v27z'],['#756244','M19 42V31q5-7 10 0v11z'],['#d8c594','M24 24V3l13 4-13 5z']],
    tower: [['#cdbd92','M12 42V15H8V7h6v5h6V7h8v5h6V7h6v8h-4v27z'],['#66573c','M21 42V30h6v12zM21 19h6v6h-6z']],
    mill: [['#d6c59a','m14 43 5-29h10l5 29z'],['#8b6941','m15 16 9-12 9 12z'],['#eee3c2','m7 4 13 15-5 4L3 8zm34 4L28 23l-5-4L36 4zM8 39l12-15 5 4-12 15zm33 0L28 24l-5 4 13 15z']],
    harbour: [['#b18a52','M6 27h36v7H6zM10 17h6v25h-6zm22 0h6v25h-6z'],['#ddd0a3','M17 21V8l13 13z']],
    barracks: [['#d1bd8b','M7 22h34v21H7z'],['#9b6f42','M3 22 13 8h24l9 14z'],['#d8c594','M19 23h12v11l-6 5-6-5z']],
  };
  const result = new Map();
  for (const [name,shapes] of Object.entries(paths)) {
    const canvas = document.createElement('canvas'); canvas.width = canvas.height = 64;
    const context = canvas.getContext('2d');
    if (!context) continue;
    context.scale(64/48,64/48); context.strokeStyle = '#302c22'; context.lineWidth = 2.5; context.lineJoin = 'round';
    for (const [colour,d] of shapes) { const path = new Path2D(d); context.fillStyle = colour; context.fill(path); context.stroke(path); }
    result.set(name,canvas);
  }
  return kind => result.get(kind === 16 ? 'castle' : [8,14].includes(kind) ? 'tower' : kind === 4 ? 'mill'
    : kind === 9 ? 'harbour' : kind === 6 ? 'barracks' : 'village');
}
