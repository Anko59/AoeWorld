import { buildingKinds, names } from './catalog.js';

const wildlife = new Set(['Sheep', 'Deer', 'Boar', 'Wolf']);
const resources = [
  ['Berry bush', 'food', 59], ['Tree', 'wood', 349], ['Gold deposit', 'gold', 66],
  ['Stone deposit', 'stone', 102], ['Shore fish', 'food', 53], ['Deep fish', 'food', 53],
];
const orderHints = {
  Villager: 'Right-click: work or move · Ctrl+right-click: queue move',
  FishingShip: 'Right-click: fish or move',
  TransportShip: 'Load at the bank · sail · unload passengers',
  General: 'Right-click: march · Alt+right-click: solo',
  Steward: 'Garrison in an owned Town Center to manage the economy',
  Vassal: 'Garrison in an owned Castle to manage its domain',
  Monk: 'Right-click: convert or move · heals nearby injured units automatically',
  Sheep: 'Right-click: herd owned sheep',
};
const isBuilding = kind => buildingKinds.includes(kind);

export function describeBuilding(card, rule, tower) {
  if (!rule) return '';
  const fields = [];
  const accepted = ['food', 'wood', 'gold', 'stone'].filter((_, index) => rule.dropOff & (1 << index));
  if (card.owned && accepted.length) {
    const resource = accepted.length === 4 ? 'resource' : card.kind === 'Dock' ? 'fish' : accepted.join(' and ');
    fields.push(resource[0].toUpperCase() + resource.slice(1) + ' drop-off');
  }
  if (rule.population) fields.push((fields.length ? 'supports ' : 'Supports ') + rule.population + ' population');
  if (rule.attack) fields.push(`Base pierce attack ${rule.attack} · range ${rule.minRange}–${rule.range}`);
  if (tower.length) fields.push(`${tower[1]}/${tower[2]} melee/pierce armour`);
  return fields.join(' · ');
}

export function describeSelection(selected, resource, unitRules = {}) {
  if (resource) {
    const [name, stock, portrait] = resources[resource.kind];
    return {
      type: 'resource', name, owner: 'Natural resource', count: 1, kind: 'Resource',
      task: resource.amount ? '' : 'Exhausted', load: resource.amount + ' ' + stock + ' remaining',
      portrait: [portrait, stock], commands: [],
    };
  }
  const first = selected[0], count = selected.length;
  if (!first) return { type: 'empty', count: 0, commands: [], name: '', hint: 'Click to select · drag to select a group' };
  const owned = selected.every(actor => actor.owned);
  const buildings = selected.filter(actor => isBuilding(actor.kind)).length;
  const isAnimal = wildlife.has(first.kind);
  const group = count > 1;
  const type = !owned && first.player >= 0 ? 'enemy' : group
    ? buildings === count ? 'buildings' : buildings ? 'mixed' : 'units'
    : isAnimal ? 'animal' : buildings ? 'building' : 'unit';
  const available = actor => actor.owned && actor.hp > 0;
  const moveable = actor => available(actor) && (actor.kind === 'Sheep' || unitRules[actor.kind]);
  const fighters = selected.filter(actor => available(actor) && unitRules[actor.kind]?.stance);
  const moves = selected.some(moveable);
  const commands = [];
  if (moves) commands.push('move-units');
  if (moves || fighters.length) commands.push('stop-units');
  if (owned && selected.some(a => available(a) && !wildlife.has(a.kind))) commands.push('delete-units');
  if (selected.some(a => available(a) && unitRules[a.kind] && a.attack > 0)) commands.push('attack-move-units');
  if (fighters.length) commands.push('patrol-units');
  if (fighters.length) commands.push('stance');
  if (selected.filter(a => available(a) && unitRules[a.kind]?.formation).length > 1) commands.push('formation');
  if (owned && count === 1 && first.kind === 'General') commands.push('general-only-move');
  const firstName = names[first.kind] ?? first.kind;
  const groupKind = buildings === count ? 'buildings' : buildings ? 'units and buildings' : 'units';
  const groupName = selected.every(a => a.kind === first.kind)
    ? firstName + ' x' + count : count + ' ' + groupKind + ' selected';
  return {
    type, commands, count, owned, kind: first.kind, first, building: buildings === count,
    villagers: selected.some(a => available(a) && a.kind === 'Villager'),
    ships: selected.some(a => available(a) && a.kind === 'FishingShip'),
    name: group ? groupName : firstName,
    owner: first.player < 0 ? 'Wildlife' : owned ? 'Your village' : 'Player ' + (first.player + 1),
    task: group ? '' : buildings ? '' : (first.task ?? '') + (first.resource && first.task === 'Gathering' ? ' ' + first.resource : ''),
    load: group || first.kind === 'Wolf' ? '' : isAnimal ? first.amount + ' food remaining'
      : first.amount ? 'Carrying ' + first.amount + '/' + first.capacity + ' ' + first.resource : '',
    hp: group ? null : first.hp, maxHp: first.maxHp,
    combat: !group && !buildings ? 'Attack ' + first.attack + ' · Armour ' + first.melee + ' / ' + first.pierce + ' · Range ' + first.range : '',
    hint: type === 'enemy' ? 'You cannot command this player’s units or buildings'
      : !owned && first.kind === 'Sheep' ? 'Right-click with a villager to lead this sheep'
      : !owned && isAnimal ? 'Select a villager to hunt this animal'
      : group ? moves ? 'Right-click to give this group an order' : ''
      : orderHints[first.kind] ?? (fighters.length ? 'Right-click: attack or move' : moves ? 'Right-click: move' : ''),
  };
}
