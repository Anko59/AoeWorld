import { set } from '/interface/dom/update.js';

function slider({ label, name = label, id, step = 1, suffix = '', input: onInput }) {
  const row = document.createElement('label');
  row.className = 'menu-slider';
  const title = document.createElement('span');
  title.textContent = label;
  const input = document.createElement('input');
  input.type = 'range';
  input.min = '0';
  input.max = '100';
  input.step = String(step);
  if (id) input.id = id;
  input.setAttribute('aria-label', name);
  const output = document.createElement('output');
  row.append(title, input, output);
  const paint = () => {
    const value = input.value + '%';
    if (input.style.getPropertyValue('--fill') !== value) input.style.setProperty('--fill', value);
    set(output, 'textContent', input.value + suffix);
  };
  input.addEventListener('input', () => {
    paint();
    onInput?.(Number(input.value));
  });
  paint();
  return {
    row, input,
    sync(value, force = false) {
      if (force || document.activeElement !== input) set(input, 'value', String(value));
      paint();
    },
  };
}

export function sliders(parent, fields) {
  const controls = fields.map(slider);
  parent.prepend(...controls.map(control => control.row));
  return controls;
}
