import { set } from '/interface/dom/update.js';

function slider({ label, name = label, hint, id, step = 1, max = 100, suffix = '', format, input: onInput, change: onChange }) {
  const row = document.createElement('label');
  row.className = 'menu-slider';
  const title = document.createElement('span');
  title.textContent = label;
  const input = document.createElement('input');
  input.type = 'range';
  input.min = '0';
  input.max = String(max);
  input.step = String(step);
  if (id) input.id = id;
  input.setAttribute('aria-label', name);
  if (hint) { row.title = hint; input.setAttribute('aria-description', hint); }
  const output = document.createElement('output');
  row.append(title, input, output);
  const paint = () => {
    const value = (Number(input.value) * 100 / max) + '%';
    if (input.style.getPropertyValue('--fill') !== value) input.style.setProperty('--fill', value);
    const shown = format ? format(Number(input.value)) : input.value + suffix;
    set(output, 'textContent', shown);
    input.setAttribute('aria-valuetext', shown);
  };
  input.addEventListener('input', () => {
    paint();
    onInput?.(Number(input.value));
  });
  input.addEventListener('change', () => onChange?.(Number(input.value)));
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
