import { icon } from '/interface/ui-art.js';

// Market rules mirror aoe-simulation's MARKET_RULES (a preview only; the server
// executes the order lot by lot and is the authority): lots of 100 units, buy at
// price +30%, sell at price -30%, every lot moves the price by 3 inside [20, 9999].
export const RULES = { lot: 100, buy: 13000, sell: 7000, step: 3, min: 20, max: 9999, maxLots: 20000, base: 100, recoverySeconds: 2 };
const resources = ['wood', 'food', 'stone'];
const asNumber = value => Number.isFinite(value) ? value : 0;
export function quote(side, price, lots, stock, gold) {
  let done = 0, total = 0, p = price;
  while (done < lots) {
    if (side === 'buy') {
      const cost = Math.floor(p * RULES.buy / 10000);
      if (gold - total < cost) break;
      total += cost; p = Math.min(RULES.max, p + RULES.step);
    } else {
      if (stock - done * RULES.lot < RULES.lot) break;
      total += Math.floor(p * RULES.sell / 10000); p = Math.max(RULES.min, p - RULES.step);
    }
    done++;
  }
  return { lots: done, gold: total, end: p };
}
// Honest about the floor: past it every lot pays the limit, which for a big sale is far below the starting price.
function floorNote(q, side, price) {
  if (q.end !== RULES.min && q.end !== RULES.max) return '';
  const limit = q.end, perLot = side === 'sell' ? Math.floor(limit * RULES.sell / 10000) : Math.floor(limit * RULES.buy / 10000);
  const steps = Math.ceil(Math.abs(limit - price) / RULES.step), atLimit = Math.max(0, q.lots - steps);
  const start = side === 'sell' ? Math.floor(RULES.base * RULES.sell / 10000) : Math.floor(RULES.base * RULES.buy / 10000);
  return ` · the price reaches its limit (${limit}) after ${steps.toLocaleString('en-US')} lots; the other ${atLimit.toLocaleString('en-US')} pay ${perLot} gold each instead of ${start}`
    + ` · it recovers 1 gold every ${RULES.recoverySeconds} s once you stop`;
}
const fmt = value => Math.round(value).toLocaleString('en-US');
export function initTrade(host) {
  const root = document.createElement('div');
  root.innerHTML = `<p class="far-hint" data-nomarket>Build a Market to trade wood, food and stone for gold.</p>
    <div data-controls><div class="far-grid" data-res style="grid-template-columns:repeat(3,minmax(0,1fr))"></div>
    <div class="far-row"><span class="far-seg" role="group" aria-label="Trade side"><button type="button" data-side="sell" aria-pressed="true">Sell</button><button type="button" data-side="buy" aria-pressed="false">Buy</button></span>
      <input class="far-input" aria-label="Units to trade" inputmode="numeric" value="10,000"> <span>units</span></div>
    <div class="far-row" data-chips></div>
    <div class="far-quote" role="status" data-quote></div>
    <div class="far-row"><button type="button" class="far-act" data-go>Trade</button></div></div>
    <p class="far-status" role="status" data-status></p>`;
  host.append(root);
  const cells = new Map();
  for (const name of resources) {
    const cell = document.createElement('button');
    cell.type = 'button'; cell.className = 'far-cell'; cell.dataset.res = name;
    cell.innerHTML = `${icon('resources', name, name)}<span class="far-cell-name">${name[0].toUpperCase() + name.slice(1)}</span><small class="far-cell-note"></small>`;
    cell.addEventListener('click', () => { state.res = name; render(); });
    root.querySelector('[data-res]').append(cell); cells.set(name, cell);
  }
  const input = root.querySelector('input'), go = root.querySelector('[data-go]'), box = root.querySelector('[data-quote]');
  const status = root.querySelector('[data-status]'), chipRow = root.querySelector('[data-chips]');
  const state = { res: 'food', side: 'sell', units: 10000, market: -1, prices: [100, 100, 100], stock: [0, 0, 0], gold: 0, mode: 'units' };
  const chips = [['1k', 1000], ['10k', 10000], ['100k', 100000], ['¼', 'quarter'], ['½', 'half'], ['All', 'all']].map(([label, value]) => {
    const chip = document.createElement('button');
    chip.type = 'button'; chip.className = 'far-chip'; chip.textContent = label; chip.dataset.chip = String(value);
    chip.addEventListener('click', () => { state.mode = typeof value === 'number' ? 'units' : value; if (typeof value === 'number') state.units = value; render(true); });
    chipRow.append(chip); return chip;
  });
  const index = () => resources.indexOf(state.res);
  // Largest affordable/sellable amount for the current side, in lots.
  function limit() {
    const price = state.prices[index()];
    return quote(state.side, price, RULES.maxLots, state.stock[index()], state.gold);
  }
  function wanted() {
    const max = limit().lots;
    if (state.mode === 'all') return max;
    if (state.mode === 'half') return Math.floor(max / 2);
    if (state.mode === 'quarter') return Math.floor(max / 4);
    return Math.min(RULES.maxLots, Math.ceil(state.units / RULES.lot));
  }
  let lots = 0;
  function render(writeInput = false) {
    const ready = state.market >= 0;
    root.querySelector('[data-nomarket]').hidden = ready; root.querySelector('[data-controls]').hidden = !ready;
    for (const [name, cell] of cells) {
      cell.setAttribute('aria-pressed', String(name === state.res));
      const note = cell.querySelector('.far-cell-note'), text = `${state.prices[resources.indexOf(name)]} g / 100`;
      if (note.textContent !== text) note.textContent = text;
    }
    root.querySelectorAll('[data-side]').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.side === state.side)));
    chips.forEach(c => c.setAttribute('aria-pressed', String(c.dataset.chip === (state.mode === 'units' ? String(state.units) : state.mode))));
    lots = wanted();
    if (writeInput || state.mode !== 'units') input.value = fmt(Math.min(lots, limit().lots) * RULES.lot || 0);
    const q = quote(state.side, state.prices[index()], lots, state.stock[index()], state.gold);
    const units = q.lots * RULES.lot, name = state.res;
    go.disabled = !ready || q.lots === 0;
    go.textContent = q.lots === 0 ? 'Nothing to trade' : `${state.side === 'buy' ? 'Buy' : 'Sell'} ${fmt(units)} ${name} for ${fmt(q.gold)} gold`;
    box.textContent = q.lots === 0 ? (state.side === 'buy' ? 'Not enough gold for one lot.' : `Not enough ${name}: one lot is 100.`)
      : `${q.lots.toLocaleString('en-US')} lots of 100 · average ${(q.gold / q.lots).toFixed(1)} gold per lot · price ${state.prices[index()]} → ${q.end}`
        + (q.lots < lots ? ` · limited by ${state.side === 'buy' ? 'gold' : name}` : '')
        + floorNote(q, state.side, state.prices[index()]);
  }
  input.addEventListener('input', () => {
    const digits = input.value.replace(/[^\d]/g, '');
    state.units = Math.min(RULES.maxLots * RULES.lot, Number(digits || 0)); state.mode = 'units'; render();
  });
  input.addEventListener('blur', () => { input.value = fmt(state.units); });
  root.querySelectorAll('[data-side]').forEach(b => b.addEventListener('click', () => { state.side = b.dataset.side; render(); }));
  go.addEventListener('click', () => {
    if (state.market < 0 || lots < 1) return;
    const code = (state.side === 'buy' ? 0 : 3) + index();
    window.aoeBuildings?.command(13, code, state.market, quote(state.side, state.prices[index()], lots, state.stock[index()], state.gold).lots || lots);
    status.textContent = `Ordered: ${state.side} ${state.res}.`;
  });
  return {
    // stock = [wood, food, gold, stone]; report = [market id, wood, food, stone price, ...].
    update(report, stock) {
      const market = Number.isFinite(report[0]) ? report[0] : -1;
      state.market = market; state.prices = [asNumber(report[1]) || 100, asNumber(report[2]) || 100, asNumber(report[3]) || 100];
      state.stock = [asNumber(stock[0]), asNumber(stock[1]), asNumber(stock[3])]; state.gold = asNumber(stock[2]);
      render();
    },
  };
}
