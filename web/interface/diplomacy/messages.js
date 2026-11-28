const byId = id => document.getElementById(id);
const key = (lo, hi) => `${hi}:${lo}`;
const requests = [
  ['ally', 'Agree to an alliance'],
  ['neutral', 'Agree to neutrality'],
  ['enemy', 'Declare hostility'],
  ['tribute', 'Send resources'],
  ['attack', 'Attack a player'],
  ['defend', 'Defend this view'],
];

function options(select, values) {
  const previous = select.value;
  select.replaceChildren(...values.map(([value, text]) => new Option(text, value)));
  if (values.some(([value]) => String(value) === previous)) select.value = previous;
}

export function diplomacyMessages(order, amount, resource, resources, panel) {
  const form = byId('diplomacy-request');
  const recipient = byId('diplomacy-recipient');
  const request = byId('diplomacy-kind');
  const attackTarget = byId('diplomacy-target');
  const list = byId('diplomacy-log');
  const notice = byId('diplomacy-notice');
  let viewer, players = new Map(), previous = '', last, timer;

  function choices() {
    const recipientId = Number(recipient.value);
    const allies = players.get(recipientId)?.stance === 2;
    const targets = [...players].filter(([id, player]) =>
      id !== viewer && id !== recipientId && player.stance !== 2);
    options(request, requests.filter(([kind]) =>
      kind === 'attack' ? allies && targets.length : !['defend', 'tribute'].includes(kind) || allies));
    options(attackTarget, targets.map(([id, player]) => [id, player.name]));
    byId('diplomacy-target-label').hidden = request.value !== 'attack';
  }

  recipient.addEventListener('change', choices);
  request.addEventListener('change', choices);
  form.addEventListener('submit', event => {
    event.preventDefault();
    const player = Number(recipient.value);
    if (recipient.value === '') return;
    const code = { enemy: 0, neutral: 1, ally: 2, attack: 3, defend: 4, tribute: 5 }[request.value];
    if (code === undefined) return;
    let count = 0;
    if (code === 5) {
      if (!amount.reportValidity()) return;
      count = amount.valueAsNumber;
    } else if (code === 3) {
      count = Number(attackTarget.value);
    }
    order(23, code === 5 ? code + Number(resource.value) : code, player, count, 'Request sent.');
  });

  function name(player) {
    return players.get(player)?.name ?? `Player ${player + 1}`;
  }

  function requestText([,,,, kind, detail, extra]) {
    switch (kind) {
      case 0: return ['Declare hostility', 'Agree to neutrality', 'Agree to an alliance'][detail];
      case 1: return `Send ${extra} ${resources[detail].toLowerCase()}`;
      case 2: return `Attack ${name(detail)}`;
      case 3: return `Help defend (${detail}, ${extra})`;
      default: return '';
    }
  }

  function text(entry, requestsById) {
    const [,, from, to, kind, detail, extra] = entry;
    if (kind === 6) return `${name(from)} surrendered to ${to === viewer ? 'you' : name(to)}.`;
    let body = requestText(entry);
    if (kind === 4 || kind === 5) {
      body = kind === 4 ? 'Accepted' : 'Declined';
      const original = requestsById.get(key(detail, extra));
      if (original) body += ` · ${requestText(original)}`;
    }
    return `${name(from)} → ${name(to)}: ${body}`;
  }

  return {
    setPlayers(local, roster) {
      viewer = local;
      players = roster;
      options(recipient, [...players].filter(([id]) => id !== viewer).map(([id, player]) => [id, player.name]));
      choices();
      if (!players.size) {
        last = undefined;
        notice.hidden = true;
        window.clearTimeout(timer);
      }
    },
    log(entries) {
      const encoded = JSON.stringify(entries);
      if (encoded === previous) return;
      previous = encoded;
      const requestsById = new Map(entries.filter(row => row[4] < 4).map(row => [key(row[0], row[1]), row]));
      const answered = new Set(entries.filter(row => row[4] === 4 || row[4] === 5).map(row => key(row[5], row[6])));
      list.replaceChildren(...entries.map(entry => {
        const [lo, hi,, to, kind] = entry;
        const li = document.createElement('li');
        const sentence = document.createElement('span');
        sentence.textContent = text(entry, requestsById);
        li.append(sentence);
        if (to === viewer && kind < 4 && !answered.has(key(lo, hi))) {
          for (const [accepted, label] of [[1, 'Accept'], [0, 'Decline']]) {
            const button = document.createElement('button');
            button.type = 'button';
            button.textContent = label;
            button.addEventListener('click', () => order(24, accepted, lo, hi, 'Reply sent.'));
            li.append(button);
          }
        }
        return li;
      }));
      list.scrollTop = list.scrollHeight;
      const newest = entries.at(-1);
      if (!newest) return;
      const id = key(newest[0], newest[1]);
      if (id !== last && newest[3] === viewer && (!panel.open || newest[4] === 6)) {
        notice.textContent = text(newest, requestsById);
        notice.hidden = false;
        window.clearTimeout(timer);
        timer = window.setTimeout(() => { notice.hidden = true; }, 8_000);
      }
      last = id;
    }
  };
}
