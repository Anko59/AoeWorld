// Compare the live DOM, so another panel or an input event can still change it.
// Reassigning an equal reflected property also mutates its attribute and can
// invalidate style/layout; equal textContent replaces retained text nodes.
export function set(node, name, value) {
  if (name === 'textContent') value = value == null ? '' : String(value);
  if (name === 'title') value = String(value);
  if (node[name] !== value) node[name] = value;
}

export function attribute(node, name, value) {
  value = String(value);
  if (node.getAttribute(name) !== value) node.setAttribute(name, value);
}
