import { entityIcon, icon } from '/interface/ui-art.js';
import { autoFit } from '/interface/selection-grid.js';
import { buildingKinds, names, techIds, techKinds, techName, trainingKinds } from './catalog.js';
import { byId, renderFields, show } from './render.js';

export function shelterRows(send) {
  const occupants = byId('garrison-units');
  autoFit(occupants, 36);
  occupants.addEventListener('click', event => {
    const unit = event.target.closest('button[data-unit-id]');
    if (unit) send(15, 4, Number(unit.dataset.unitId));
  });
  let state = {}, previous = '';
  window.aoeGarrison = {
    update(values) {
      const [capacity, enter, bell, backToWork] = values;
      const count = Math.max(0, (values.length - 4) / 4);
      state = { capacity, enter: enter === 1, bell: bell === 1, backToWork: backToWork === 1, count };
      renderFields({ 'garrison-panel': { hidden: !capacity }, 'garrison-count': { textContent: `${count} / ${capacity}` } });
      const signature = values.slice(4).join(',');
      if (signature === previous) return;
      previous = signature;
      let markup = '';
      for (let at = 4; at < values.length; at += 4) {
        const [id, unit, hp, maxHp] = values.slice(at, at + 4);
        const kind = trainingKinds[unit], label = `Ungarrison ${names[kind]} · ${hp} / ${maxHp} HP`;
        markup += `<button type="button" data-unit-id="${id}" title="${label}" aria-label="${label}">${entityIcon(kind)}</button>`;
      }
      renderFields({ 'garrison-units': { innerHTML: markup } });
    },
  };
  return () => state;
}

const queueIcon = unit => {
  const kind = trainingKinds[unit];
  return `<span class="unit-token" title="${names[kind]} queued">${entityIcon(kind)}</span>`;
};

function productionFields(state, card, queue) {
  const { progress, researching, researchProgress, waiting, food, hp, maxHp, market, reseed } = state;
  const complete = progress >= 1000;
  const researchPercent = Math.floor(researchProgress * 100);
  const { elapsed, total, units } = queue ?? { units: [] };
  const trainee = names[trainingKinds[units[0]]];
  const percent = Math.floor(elapsed / Math.max(1, total) * 100);
  card.task = !complete ? 'Under construction' : waiting ? 'Waiting for housing' :
    researching >= 0 ? 'Researching' : trainee ? 'Training ' + trainee : 'Ready';
  if (card.owned) {
    if (!complete) card.hint = 'Right-click this foundation with villagers to resume construction';
    if (card.kind === 'Farm') card.hint = 'Right-click this Farm with a villager to work it';
  }
  return {
    'building-hp': { textContent: `${hp} / ${maxHp} HP` },
    'building-status': { textContent: !complete ? `Constructing · ${Math.floor(progress / 10)}%` :
      food >= 0 ? `Complete · ${food} food remaining` : 'Complete' },
    'construction-progress': { hidden: complete, value: progress },
    'research-icon': { innerHTML: researching >= 0 ? icon('technologies', techIds[researching], 'technology') : '' },
    'research-label': { textContent: researching >= 0 ? `${techName(techKinds[researching])} · ${researchPercent}%` : '' },
    'research-progress': { hidden: researching < 0, value: researchPercent },
    'market-prices': { textContent: card.owned && market ?
      `Prices per 100: wood ${market[0]} · food ${market[1]} · stone ${market[2]} gold` : '' },
    'farm-reseed-queue': { hidden: !card.owned || !reseed },
    'farm-reseed-count': { textContent: reseed?.count },
    'training-label': trainee ? { textContent: `${waiting ? 'Waiting for housing · ' : 'Training '}${trainee} · ${percent}%` } : {},
    'training-progress': trainee ? { value: percent, 'aria-label': trainee + ' training progress' } : {},
  };
}

export function productionRows() {
  const list = byId('training-multi');
  const [host, pager] = list.children;
  const queue = byId('training-queue');
  autoFit(queue);
  let page = 0, queueKey = '', groupsKey = '', groups = [];
  function drawGroups() {
    const pages = Math.ceil(groups.length / 3);
    page = Math.min(page, Math.max(0, pages - 1));
    pager.hidden = pages <= 1;
    pager.textContent = `${page + 1} / ${pages} ›`;
    host.innerHTML = groups.slice(page * 3, page * 3 + 3).map(([id, kind, units], i) => {
      const site = names[buildingKinds[kind]];
      const remaining = units.length > 6 ? ` +${units.length - 6}` : '';
      return `<div class="training-row">
        <span class="training-site" title="${site} · ${id}">${site} ${page * 3 + i + 1}</span>
        <span class="training-tokens" aria-label="${units.map(unit => names[trainingKinds[unit]]).join(', ')}">${units.slice(0, 6).map(queueIcon).join('') || 'Empty'}${remaining}</span>
      </div>`;
    }).join('');
  }
  pager.addEventListener('click', () => {
    page = (page + 1) % Math.ceil(groups.length / 3);
    drawGroups();
  });
  return (state, card, { queue: training, trainingRows }) => {
    const building = state.kind >= 0 && card.building;
    const single = building && card.count === 1;
    const activeQueue = single && card.owned ? training : null;
    show('building-panel', single);
    show('training', Boolean(activeQueue?.units.length));
    if (single) renderFields(productionFields(state, card, activeQueue));
    const units = activeQueue?.units ?? [], key = units.join(',');
    if (queueKey !== key) {
      queueKey = key;
      queue.innerHTML = units.map(queueIcon).join('');
    }
    groups = building && card.owned && card.count > 1 ? trainingRows : [];
    show('training-multi', groups.length > 0);
    const groupKey = JSON.stringify(groups);
    if (groupsKey !== groupKey) {
      groupsKey = groupKey;
      drawGroups();
    }
  };
}
