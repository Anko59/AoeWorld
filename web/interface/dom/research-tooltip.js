let tooltip, anchor;

function hide() {
  if (anchor) tooltip.hidePopover();
  anchor = null;
}

function show(button) {
  const detail = button.getAttribute('aria-description');
  if (!detail || !button.getClientRects().length) return;
  if (!tooltip) {
    tooltip = document.createElement('div');
    tooltip.id = 'research-tooltip';
    tooltip.setAttribute('popover', 'manual');
    tooltip.setAttribute('role', 'tooltip');
    document.body.append(tooltip);
    document.addEventListener('keydown', event => { if (event.key === 'Escape') hide(); });
    document.addEventListener('scroll', hide, true);
    window.addEventListener('resize', hide);
  }
  tooltip.textContent = detail;
  if (!anchor) tooltip.showPopover();
  anchor = button;
  const box = button.getBoundingClientRect();
  const { width, height } = tooltip.getBoundingClientRect();
  const left = Math.max(8, Math.min(box.left, window.innerWidth - width - 8));
  const top = box.top >= height + 8 ? box.top - height - 6 : box.bottom + 6;
  tooltip.style.left = `${left}px`;
  tooltip.style.top = `${top}px`;
}

export function bindResearchTooltip(button) {
  for (const event of ['pointerenter', 'focus']) {
    button.addEventListener(event, () => show(button));
  }
  for (const event of ['pointerleave', 'blur']) {
    button.addEventListener(event, () => { if (anchor === button) hide(); });
  }
}

export function updateResearchTooltip(button, detail) {
  if (button.getAttribute('aria-description') !== detail) {
    button.setAttribute('aria-description', detail);
    if (anchor === button) show(button);
  }
  if (anchor === button && !button.getClientRects().length) hide();
}
