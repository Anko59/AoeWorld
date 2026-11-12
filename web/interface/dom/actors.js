import { set } from '/interface/dom/update.js';

const style = (node, name, value) => {
  if (node.style[name] !== value) node.style[name] = value;
};
const span = (className, content = '') => {
  const node = document.createElement('span');
  if (className) node.className = className;
  node.textContent = content;
  return node;
};

// Actor ids retain their labels across motion; dropping an actor releases its
// node. Keep stream order too, including when an earlier actor enters view.
function layer(id, create, update) {
  const container = document.getElementById(id), nodes = new Map();
  return actors => {
    const live = new Set();
    let next = container.firstChild;
    for (const actor of actors) {
      live.add(actor.id);
      let node = nodes.get(actor.id);
      if (!node) { node = create(actor); nodes.set(actor.id, node); }
      if (node !== next) container.insertBefore(node, next);
      next = node.nextSibling;
      update(node, actor);
    }
    for (const [id, node] of nodes) if (!live.has(id)) { node.remove(); nodes.delete(id); }
  };
}

export function actorLabels(icon) {
  const health = layer('unit-health-bars', () => {
    const node = span('unit-hp-bar'); node.append(span('')); return node;
  }, (node, actor) => {
    style(node, 'left', `${actor.x - 18}px`); style(node, 'top', `${actor.head}px`);
    style(node.firstChild, 'width', `${100 * actor.hp / actor.maxHp}%`);
  });
  const idle = layer('idle-markers', () => span('idle-marker', 'z'), (node, actor) => {
    style(node, 'left', `${actor.x - 6}px`); style(node, 'top', `${actor.head - 16}px`);
  });
  const flags = layer('garrison-flags', actor => {
    const node = span('garrison-flag'); node.dataset.buildingId = String(actor.id); return node;
  }, (node, actor) => {
    set(node, 'title', `${actor.garrison} garrisoned`); set(node, 'textContent', actor.garrison);
    style(node, 'left', `${actor.x}px`); style(node, 'top', `${actor.y - 48}px`);
  });
  const kinds = new WeakMap();
  const loads = layer('carried-loads', () => span('load-badge'), (node, actor) => {
    if (kinds.get(node) !== actor.load[0]) {
      node.innerHTML = icon(actor.load[0]); node.append(document.createTextNode(''));
      kinds.set(node, actor.load[0]);
    }
    set(node.lastChild, 'textContent', actor.load[1]);
    style(node, 'left', `${actor.x + 12}px`); style(node, 'top', `${actor.y - 36}px`);
  });
  return { health, idle, flags, loads };
}
