import { diplomacyMessages } from './messages.js';
import { icon } from '/interface/ui-art.js';
import { actorColour } from '/minimap.js';

const stances = ['Enemy', 'Neutral', 'Ally'];
const resources = ['Food', 'Wood', 'Gold', 'Stone'];
const byId = id => document.getElementById(id);
const element = (tag, text) => {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  return node;
};

export function initDiplomacy() {
  const panel = byId('diplomacy-panel');
  const toggle = byId('diplomacy-toggle');
  const body = byId('diplomacy-rows');
  const scores = byId('scoreboard');
  const roster = byId('scoreboard-players');
  const markets = byId('minimap-allied-markets');
  const fee = byId('tribute-fee');
  const result = byId('diplomacy-result');
  const amount = byId('tribute-amount');
  const resource = byId('tribute-resource');
  let pending, previous = '';
  for (const [index, name] of resources.entries()) {
    const option = element('option', name);
    option.value = index;
    resource.append(option);
  }
  const updateResourceIcon = () => {
    const name = resources[Number(resource.value)].toLowerCase();
    byId('tribute-resource-icon').innerHTML = icon('resources', name, name);
  };
  resource.addEventListener('change', updateResourceIcon);
  updateResourceIcon();
  toggle.addEventListener('click', () => {
    window.dispatchEvent(new Event('blur'));
    panel.showModal();
  });
  byId('diplomacy-close').addEventListener('click', () => panel.close());
  panel.addEventListener('close', () => byId('scene').focus());

  const order = (action, code, player, count, message) => {
    if (pending) return;
    pending = { action, message };
    result.textContent = 'Sending…';
    window.aoeBuildings.command(action, code, player, count);
  };
  const messages = diplomacyMessages(order, amount, resource, resources, panel);
  function row(player, stance, offer, colour, name, hasMarket) {
    const line = element('tr');
    const label = element('th', name);
    label.scope = 'row';
    label.className = 'diplomacy-name';
    label.style.setProperty('--seat-colour', actorColour(colour));
    line.append(label);
    const relation = element('td');
    const choice = element('select');
    choice.setAttribute('aria-label', `Stance toward ${name}`);
    for (const value of [2, 1, 0]) {
      const option = element('option', stances[value]);
      option.value = value;
      choice.append(option);
    }
    choice.value = stance;
    choice.addEventListener('change', () => {
      const value = Number(choice.value);
      choice.value = stance;
      order(21, value, player, 0, value === 2 ? `Alliance offered to ${name}.` : `Stance set to ${stances[value]}.`);
    });
    relation.append(choice);
    if (offer) {
      const text = offer === 1 ? 'Offer sent' : 'Accept alliance';
      const badge = offer === 1 ? element('small', text) : element('button', text);
      if (offer === 2) badge.addEventListener('click', () => order(21, 2, player, 0, `Allied with ${name}.`));
      relation.append(badge);
    }
    const tribute = element('td');
    const send = element('button', 'Send');
    send.type = 'button';
    send.disabled = !hasMarket;
    send.setAttribute('aria-label', `Send tribute to ${name}`);
    send.addEventListener('click', () => {
      if (!amount.reportValidity()) return;
      const count = amount.valueAsNumber;
      const kind = Number(resource.value);
      order(22, kind, player, count, `Sent ${count} ${resources[kind].toLowerCase()} to ${name}.`);
    });
    tribute.append(send);
    line.append(relation, tribute);
    return line;
  }

  window.aoeDiplomacy = {
    update(info, names, marks) {
      const [viewer, hasMarket, rate, width, height] = info;
      const key = `${info.join(',')}/${JSON.stringify(Array.from(names))}`;
      toggle.disabled = names.length < 2;
      scores.hidden = names.length < 2;
      if (!names.length) {
        panel.close();
        result.textContent = '';
        pending = undefined;
      }
      if (key !== previous) {
        previous = key;
        fee.textContent = hasMarket ? `Tribute fee ${rate / 100}%` : 'Tribute requires a completed Market.';
        const rows = [];
        const players = [];
        const seats = new Map();
        for (let i = 0; i < names.length; i++) {
          const [player, stance, offer, colour] = info.subarray(5 + i * 4, 9 + i * 4);
          const name = player === viewer ? 'You' : names[i] ?? `Player ${player + 1}`;
          seats.set(player, { name, stance });
          const item = element('li', name);
          item.className = 'diplomacy-name';
          item.style.setProperty('--seat-colour', actorColour(colour));
          players.push(item);
          if (player !== viewer) rows.push(row(player, stance, offer, colour, name, hasMarket));
        }
        messages.setPlayers(viewer, seats);
        body.replaceChildren(...rows);
        roster.replaceChildren(...players);
      }
      let path = '';
      for (let i = 0; i + 2 < marks.length; i += 3) {
        const x = marks[i + 1] / width * 256;
        const y = marks[i + 2] / height * 256;
        path += `M${x} ${y - 2.5}l2.5 2.5 -2.5 2.5 -2.5 -2.5z`;
      }
      if (markets.getAttribute('d') !== path) markets.setAttribute('d', path);
    },
    log(entries) { messages.log(entries); },
    result(accepted) {
      if (!pending) return;
      result.textContent = accepted ? pending.message
        : pending.action === 21 ? 'Diplomacy order refused. End fighting before offering an alliance.'
          : pending.action === 22 ? 'Tribute refused. Check your Market and available resources.'
            : 'This diplomatic request could not be completed.';
      pending = undefined;
    }
  };
}
