import { set, attribute } from '/interface/dom/update.js';
import { transportLoads } from '/interface/navy/indicators.js';
import { artReady } from '/interface/ui-art.js';
import { buildableKinds, buildingKinds, names, price, techIds, techKinds, techName, trainingKinds } from './menu/catalog.js';
import { describeBuilding, describeSelection } from './menu/selection.js';
import { describeCommands } from './menu/commands.js';
import { byId, commandGrid, renderFields, selectionCard, show, text } from './menu/render.js';
import { productionRows, shelterRows } from './menu/production.js';

const techCount = techKinds.length, buildCount = buildableKinds.length;
const buildStart = 8 + techCount, foundationCount = buildStart + buildCount;
const ageNames = ['Dark', 'Feudal', 'Castle', 'Imperial'];

function readCatalog(values) {
  let at = 0;
  const next = size => {
    const row = Array.from(values.subarray(at, at + size));
    at += size;
    return row;
  };
  const buildings = buildingKinds.map(() => {
    const [age, buildable, required, population, dropOff] = next(5);
    const cost = next(4), [attack, minRange, range] = next(3);
    return { age, buildable: buildable === 1, requires: required < 0 ? undefined : required, population, dropOff, cost, attack, minRange, range };
  });
  const techs = techKinds.map((kind, index) => {
    const [site, slot, ...cost] = next(6);
    return { index, label: [techName(kind), price(cost)].filter(Boolean).join(' · '), site, slot, art: techIds[index], cost };
  });
  const units = trainingKinds.map(() => {
    const [formation, stance, slot, ...cost] = next(7);
    return { formation: formation === 1, stance: stance === 1, slot, cost };
  });
  return { buildings, techs, units, unitRules: Object.fromEntries(trainingKinds.map((kind, index) => [kind, units[index]])) };
}

function readBuildingState(v, tower) {
  const [age, kind, hp, maxHp, progress, food, researching, researchProgress] = v;
  const current = v[foundationCount] ?? 0;
  const from = foundationCount + 1;
  const train = from + current * 3, market = train + trainingKinds.length + 1;
  const reseed = market + 15;
  return {
    age, kind, tower: Array.from(tower), hp, maxHp, progress, food, researching, researchProgress,
    researchAvailability: Array.from(v.subarray(8, buildStart)),
    build: v.subarray(buildStart, foundationCount),
    foundations: v.subarray(from, train),
    training: Array.from(v.subarray(train, market - 1)),
    primary: v[market - 1],
    market: buildingKinds[kind] === 'Market' && progress >= 1000 ? v.subarray(market, reseed) : null,
    reseed: buildingKinds[kind] === 'Mill' ? { count: v[reseed], add: v[reseed + 1] === 1, remove: v[reseed + 2] === 1, cost: v[reseed + 3] } : null,
    scouts: v[reseed + 4], scouting: v[reseed + 5],
    cargo: v[reseed + 6] ? v[reseed + 7] : null,
    transport: v.subarray(reseed + 8, -1),
    waiting: v[v.length - 1] === 1,
  };
}

function drawFoundations(positions) {
  let labels = '';
  for (let i = 0; i < positions.length; i += 3) {
    const [x, y, progress] = positions.subarray(i, i + 3);
    labels += `<span style="left:${x}px;top:${y}px">${Math.floor(progress / 10)}%<progress max="1000" value="${progress}"></progress></span>`;
  }
  renderFields({ 'construction-labels': { innerHTML: labels } });
}

export function initBuildings() {
  const send = (...args) => window.aoeBuildings?.command?.(...args);
  const shelter = shelterRows(send);
  const drawCard = selectionCard();
  byId('selected-icons').addEventListener('click', event => {
    const unit = event.target.closest('.unit-token[data-unit-id]');
    if (!unit) return;
    const grouped = event.ctrlKey || unit.classList.contains('unit-group');
    send(16, Number(event.shiftKey) + 2 * Number(grouped), Number(unit.dataset.unitId));
  });
  for (const [code, kind] of ['villagers', 'military'].entries()) {
    byId('idle-' + kind).addEventListener('click', () => send(17, code));
  }
  let page = 'closed', commandPage = 0, formation = 0, selectionKey = '';
  let selection = { selected: [], queue: null, trainingRows: [] }, resource = null;
  let state, rules, previewScene, lastPreview, rendered = '', callback;
  const dispatch = (order, shift) => {
    if (Array.isArray(order)) {
      const [action, code] = order;
      if (action === 5) formation = code;
      callback?.(action, code, 0, shift ? 5 : 1);
    } else if (order === 'next' || order === 'previous') {
      commandPage += order === 'next' ? 1 : -1;
    } else {
      page = page === order ? 'closed' : order;
      commandPage = 0;
      if (page === 'closed') callback?.(11, 0, 0, 1);
    }
    render();
  };
  const grid = commandGrid(dispatch), rows = productionRows();
  let foundationKey = '';
  artReady.then(() => { rendered = ''; render(); });

  function render() {
    if (!state || !rules) return;
    const scene = byId('scene');
    const { selected, queue, trainingRows } = selection;
    const key = selected.map(unit => unit.id).join(',');
    if (key !== selectionKey) {
      selectionKey = key;
      page = 'closed';
      commandPage = 0;
    }
    const garrison = shelter(), stance = scene.dataset.selectedStance;
    const signature = JSON.stringify([selected, queue, trainingRows, resource, state,
      garrison, stance, page, commandPage, formation, scene.dataset.orderMode]);
    if (signature === rendered) return;
    rendered = signature;
    const card = describeSelection(selected, resource, rules.unitRules);
    rows(state, card, selection);
    if (card.building && card.count === 1) card.load = describeBuilding(card, rules.buildings[state.kind], state.tower);
    if (state.cargo !== null) card.load = state.cargo + '/5 passengers';
    drawCard(card, selected);
    const commands = describeCommands(card, state, garrison, rules,
      { page, formation, stance, training: card.count === 1 && queue?.units.length });
    commandPage = grid.update(commands, commandPage);
    const open = page !== 'closed';
    text('command-context', open ? (page === 'military' ? 'Military buildings' : 'Economic buildings')
      : card.name || 'Select a unit');
    text('command-hint', open && scene.dataset.orderMode === 'build'
      ? scene.title || 'Left-click a green footprint · drag walls or bridges bank to bank · Esc cancels'
      : open ? 'Choose a building · Esc closes the menu' : card.hint || '');
  }
  window.aoeBuildings = {
    techs: [],
    catalog(values) { rules = readCatalog(values); this.techs = rules.techs; },
    selection(value) { selection = value; },
    resource(value) { resource = value; },
    bind(send) { callback = send; this.command = send; },
    key: grid.key,
    ageProgress(age, fraction) {
      const bar = byId('age-progress');
      show('age-progress', Boolean(age));
      if (!age) return;
      const percent = Math.floor(Math.min(1, Math.max(0, fraction)) * 100);
      if (bar.style.getPropertyValue('--progress') !== percent + '%') bar.style.setProperty('--progress', percent + '%');
      set(bar.querySelector('small'), 'textContent', percent + '%');
      attribute(bar, 'aria-valuenow', percent);
      set(bar, 'title', 'Advancing to ' + ageNames[age] + ' Age · ' + percent + '%');
    },
    update(values, tower = []) {
      // The WASM slice expires when this callback returns. Keyboard and button
      // actions redraw from this small, browser-owned snapshot.
      const { foundations, transport, ...panel } = readBuildingState(values.slice(), tower);
      state = panel;
      window.aoeNavigation?.strategic?.building?.ready(state.build);
      text('age-name', ['I', 'II', 'III', 'IV'][state.age] + '  ' + ageNames[state.age] + ' Age');
      const next = foundations.join(',');
      if (next !== foundationKey) {
        foundationKey = next;
        drawFoundations(foundations);
      }
      transportLoads(transport);
      render();
    },
    preview(values) {
      const scene = byId('scene');
      const signature = values.join(',');
      if (scene === previewScene && signature === lastPreview) return;
      previewScene = scene;
      lastPreview = signature;
      attribute(scene, 'data-placement-valid', values[0] === 1);
      attribute(scene, 'data-placement-count', values[1] ?? 0);
      attribute(scene, 'data-placement-invalid', values[2] ?? 0);
      const reasons = ['', 'Drag between two land banks', 'Bridge is too long or has an invalid direction',
        'Bridge start must be on land', 'Bridge end must be on land', 'Not enough resources for this bridge',
        'Bridge tiles must be clear and cross water'];
      set(scene, 'title', reasons[values[3] ?? 0] ?? '');
      if (page !== 'closed' && scene.dataset.orderMode === 'build') text('command-hint', scene.title ||
        'Left-click a green footprint · drag walls or bridges bank to bank · Esc cancels');
    },
  };
}
