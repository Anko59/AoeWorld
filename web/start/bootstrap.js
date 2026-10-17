import init from '/pkg/aoe_client.js';
import { initInterface } from '/interface/hud.js';
import { initNavigation } from '/navigation.js';
import { initStartSequence } from '/start/sequence.js';
import { initMapJobs } from '/map-jobs.js';
initInterface();
initNavigation();
const readJson = async response => {
  if (!response.ok) throw new Error(await response.text());
  return response.json();
};
const controllerHeaders = () => {
  const token = sessionStorage.getItem('aoeworld.resume-token');
  if (!/^[0-9a-f]{48}$/i.test(token || '')) throw new Error('Connect to your world before preparing a map.');
  return { 'X-AoeWorld-Controller-Token': token };
};
const sequence = initStartSequence({ readJson, start: request => jobs.start(request), cancel: () => jobs.cancel() });
window.aoeStart = sequence;
const jobs = initMapJobs({ readJson, controllerHeaders,
  onStart: sequence.begin, onProgress: sequence.progress, onActivated: sequence.activated,
  onError: sequence.error, onSettled: () => { sequence.recover(jobs.pending()); sequence.settled(); },
  onCancelable: sequence.cancelable, onCancelled: sequence.cancelled,
  onRecovering: value => sequence.recover(jobs.pending(), value) });
sequence.recover(jobs.pending());
// Recovery waits for the authoritative socket and controller role. A retained
// submission is probed once per load, with its original idempotency key.
const recoveryObserver = new MutationObserver(recoverWhenReady);
function recoverWhenReady() {
  if (document.getElementById('scene').dataset.connected !== 'true'
      || !document.getElementById('observer-label').hidden) return;
  recoveryObserver.disconnect();
  jobs.recover();
}
recoveryObserver.observe(document.getElementById('scene'), { attributes: true, attributeFilter: ['data-connected'] });
recoveryObserver.observe(document.getElementById('observer-label'), { attributes: true, attributeFilter: ['hidden'] });
recoverWhenReady();
init().catch(error => {
  document.getElementById('connection').textContent = 'Unavailable';
  document.getElementById('unsupported').textContent = `Unable to start AoeWorld: ${error}`;
  sequence.error('The game could not connect. Reload the page to try again.');
});
