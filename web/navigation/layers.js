// These keys are reserved only while the strategic scene is on screen.
export function initLayers(changed) {
  const control = document.createElement('nav');
  control.id = 'strategic-layers';
  control.setAttribute('aria-label', 'Map layers');
  control.hidden = true;
  document.body.append(control);
  const names = ['Ownership', 'Armies', 'Economy', 'Threats', 'Resources', 'Roads & bridges'];
  const keys = ['ownership', 'armies', 'economy', 'threats', 'resources', 'roads'];
  const enabled = Object.fromEntries(keys.map(key => [key, true]));
  const buttons = keys.map((key, i) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = `${names[i]} F${i + 1}`;
    button.title = key === 'ownership' ? 'Building plots and active Town Center work domains; political territory unavailable'
      : key === 'roads' ? 'Known roads and bridges'
      : key === 'threats' ? 'Last seen enemies, up to two minutes old; never live positions under fog'
      : `Toggle ${names[i].toLowerCase()}`;
    button.setAttribute('aria-pressed', 'true');
    button.addEventListener('click', () => toggle(i));
    control.append(button);
    return button;
  });
  function toggle(i) {
    const key = keys[i];
    enabled[key] = !enabled[key];
    buttons[i].setAttribute('aria-pressed', String(enabled[key]));
    changed();
  }
  document.addEventListener('keydown', event => {
    if (control.hidden || event.defaultPrevented || event.repeat || event.ctrlKey || event.shiftKey || event.altKey || event.metaKey
      || document.querySelector('dialog[open]')
      || (event.target instanceof Element && event.target.closest('input,textarea,select,[contenteditable]'))) return;
    const i = ['F1', 'F2', 'F3', 'F4', 'F5', 'F6'].indexOf(event.key);
    if (i < 0) return;
    event.preventDefault();
    toggle(i);
  });
  return { enabled, show(value) { if (control.hidden !== !value) control.hidden = !value; } };
}
