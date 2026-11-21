import { buildableKinds, buildingKinds, id, names, price, techKinds, techName, trainingKinds } from './catalog.js';

export const gridKeys = 'QWERTASDFGZXCVB';
// Grid placement is presentation metadata. Availability, cost and the set of
// constructible buildings always come from the current catalog and HUD state.
const buildSlots = {
  economy: { House: 'Q', Mill: 'W', MiningCamp: 'E', LumberCamp: 'R', Dock: 'T',
    Farm: 'A', Blacksmith: 'S', Market: 'D', Bridge: 'G', Monastery: 'Z', TownCenter: 'C', University: 'X' },
  military: { Barracks: 'Q', ArcheryRange: 'W', Stable: 'E', Outpost: 'A', PalisadeWall: 'S',
    SiegeWorkshop: 'D', WatchTower: 'F', StoneGate: 'G', StoneWall: 'Z', PalisadeGate: 'X',
    Castle: 'C', Road: 'R', Wonder: 'T' },
};
const disabledReasons = ['Unavailable', '', 'requires Feudal Age', 'Already researched',
  'Research in progress', 'Wait for the training queue to finish', 'Not enough food', 'Not enough wood',
  'Not enough gold', 'Not enough stone', 'Training queue is full', 'Building is under construction',
  'Requires prerequisite buildings or technology', 'Select your own building', 'requires Castle Age',
  'A Steward is queued or garrisoned in this Town Center', 'requires Imperial Age'];

const action = (id, label, icon, order, key, props) => ({ id, label, icon, order, key, ...props });
const command = (id, label, art, order, key, props) => action(id, label, ['commands', ...art], order, key, props);
const entity = (id, label, kind, order, key, props) => action(id, label, ['entity', kind], order, key, props);
const withCode = code => ({ disabled: code !== 1, reason: disabledReasons[code] });
const byLine = (a, b) => a.slot - b.slot;
const formations = ['Line', 'Box', 'Staggered', 'Flank'];
const stances = ['Aggressive', 'Defensive', 'Stand Ground', 'No Attack'];
const ageNames = ['Dark Age', 'Feudal Age', 'Castle Age', 'Imperial Age'];

export function describeCommands(card, state, shelter, rules, { page, stance, formation, training }) {
  const militaryCommand = state.scouts ? [command('auto-scout',
    state.scouting === state.scouts ? 'Stop auto-scout' : 'Auto-scout', ['auto-scout', 'scout'],
    [0, buildableKinds.length + techKinds.length + 4], 'E', {
      pressed: state.scouting === 0 ? 'false' : state.scouting === state.scouts ? 'true' : 'mixed',
    })] : [];
  const controls = {
    'move-units': [command('move-units', 'Move · choose a destination', ['move', 'move'], [3, 2], 'Q')],
    'stop-units': [command('stop-units', 'Stop selected units', ['stop', 'stop'], [3, 1], 'G')],
    'delete-units': [command('delete-units', 'Delete', ['delete', 'delete'], [3, 4], 'Delete', { title: 'Delete (Del)' })],
    'attack-move-units': [command('attack-move-units', 'Attack move', ['attack-move', 'soldier'], [6, 0], 'R')],
    'patrol-units': [command('patrol-units', 'Patrol between this position and the destination', ['patrol', 'move'], [6, 1], '')],
    'general-only-move': [entity('general-only-move', 'General only · move without the army', 'General', [8, 0], 'W')],
    formation: formations.map((name, index) =>
      command('formation-' + id(name), name + ' formation', ['formation-' + id(name), 'soldier'],
        [5, index], 'ZXCV'[index], { pressed: String(formation === index), className: 'formation-command' })),
    stance: stances.map((name, index) =>
      command('stance-' + id(name), name, ['stance-' + id(name), 'soldier'],
        [7, index], 'ASDF'[index], { pressed: String(String(index) === stance) })),
  };
  const general = card.commands.flatMap(key => controls[key]);
  if (card.villagers) {
    const buildTabs = [
      command('build-menu', 'Economic buildings', ['build', 'towncenter'], 'economy', 'B'),
      command('build-military-menu', 'Military buildings', ['military', 'soldier'], 'military', 'V'),
    ];
    if (page === 'economy' || page === 'military') {
      const options = buildableKinds.flatMap((kind, index) => {
        const group = kind in buildSlots.military ? 'military' : 'economy';
        if (kind === 'FishTrap' || !rules.buildings[index]?.buildable || group !== page) return [];
        return [construction(kind, (index + buildableKinds.length - 1) % buildableKinds.length, state, rules, buildSlots[page][kind])];
      });
      return [...options, buildTabs[page === 'economy' ? 1 : 0],
        command('close-build-menu', 'Close building menu', ['cancel', 'delete'], 'closed', 'Escape')];
    }
    general.unshift(...buildTabs);
  }
  if (card.ships) general.unshift(construction('FishTrap', buildableKinds.indexOf('FishTrap') - 1, state, rules, 'F'));
  if (shelter.enter) general.push(command('garrison', 'Garrison · choose a friendly shelter', ['garrison', 'towncenter'], [15, 0], 'T'));
  general.push(...militaryCommand);
  if (state.cargo !== null) general.push(command('unload-transport', 'Unload passengers at a shore', ['ungarrison', 'move'], [4, 0], 'W', { disabled: state.cargo === 0 }));
  if (!card.owned || !card.building || state.kind < 0) return general;
  if (shelter.capacity) general.push(
    command('ungarrison', 'Ungarrison all', ['ungarrison', 'move'], [15, 1], 'G', { disabled: !shelter.count }),
    command('back-to-work', 'Back to Work', ['back-to-work', 'villager'], [15, 2], 'F', { disabled: !shelter.backToWork }),
  );
  if (shelter.bell) general.push(command('town-bell', 'Town Bell', ['town-bell', 'towncenter'], [15, 3], 'B'));
  const end = buildableKinds.length + techKinds.length;
  if (state.progress < 1000) return [...general,
    command('cancel-construction', 'Cancel construction', ['cancel', 'delete'], [0, end], '')];
  if (state.researching >= 0) general.push(command('cancel-research', 'Cancel research',
    ['cancel', 'delete'], [0, end + 1], ''));
  if (training) general.push(command('cancel-training', 'Cancel',
    ['cancel', 'delete'], [3, 3], '', { title: 'Cancel active training and refund its cost' }));
  const train = state.training.flatMap((code, index) => {
    if (!code) return [];
    const kind = trainingKinds[index];
    const primary = state.primary === index;
    const { cost, slot } = rules.units[index];
    return [entity(primary ? 'train-villager' : 'train-' + id(kind), 'Train ' + names[kind],
      kind, [1, index], primary ? 'Q' : '', { slot, cost: price(cost), ...withCode(code) })];
  }).sort(byLine);
  const research = state.researchAvailability.flatMap((code, index) => rules.techs[index].site !== state.kind ? [] : [
    action('research-' + id(techKinds[index]), 'Research ' + techName(techKinds[index]),
      ['technologies', rules.techs[index].art, 'technology'], [0, buildableKinds.length + index],
      '', { slot: rules.techs[index].slot, cost: price(rules.techs[index].cost), ...withCode(code),
        hidden: code === 0 || code === 3, className: 'research-command', research: true,
        tooltip: rules.techs[index].tooltip }),
  ]).sort(byLine);
  return [...train, ...research, ...buildingActions(state), ...general];
}

function construction(kind, index, state, rules, key) {
  const rule = rules.buildings[buildingKinds.indexOf(kind)];
  const required = rule?.requires === undefined ? '' : names[buildingKinds[rule.requires]] ?? '';
  const cost = rule ? [price(rule.cost), required && 'needs ' + required].filter(Boolean).join(' · ') : '';
  const enabled = state.build[index] === 1;
  const reason = enabled ? '' : rule && rule.age > state.age ? 'requires ' + ageNames[rule.age] : 'Requirements or resources unavailable';
  return entity('build-' + id(kind), names[kind], kind, [0, index], key,
    { cost, disabled: !enabled, reason, className: 'build-command' });
}

function buildingActions(state) {
  const commands = [];
  const end = buildableKinds.length + techKinds.length;
  if (state.reseed !== null) {
    commands.push(
      entity('add-farm-reseed', 'Queue farm reseed', 'Farm', [0, end + 2], '',
        { disabled: !state.reseed.add, cost: state.reseed.cost + ' wood' }),
      entity('remove-farm-reseed', 'Remove farm reseed', 'Farm', [0, end + 3], '',
        { disabled: !state.reseed.remove, cost: 'refund ' + state.reseed.cost + ' wood' }),
    );
  }
  if (state.market) {
    for (const [index, resource] of ['wood', 'food', 'stone'].entries()) {
      for (const [offset, side, sign] of [[0, 'Buy', '+'], [3, 'Sell', '−']]) {
        const value = state.market[9 + offset + index];
        commands.push(action('market-' + side.toLowerCase() + '-' + resource,
          side + ' 100 ' + resource + ' for ' + value + ' gold', ['resources', resource, resource],
          [2, offset + index], '', { badge: sign, price: state.market[index], disabled: state.market[3 + offset + index] !== 1 }));
      }
    }
  }
  return commands;
}
