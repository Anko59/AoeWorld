export const colours = [
  ['blue', '#0000ff'], ['red', '#ff0000'], ['green', '#00ff00'], ['yellow', '#ffff00'],
  ['cyan', '#00ffff'], ['purple', '#ff00ff'], ['grey', '#808080'], ['orange', '#ff8000'],
];

export const presets = [
  ['paris', 'Paris basin', 'Rivers & open country', 48.8566, 2.3522],
  ['alps', 'Alps', 'Peaks & mountain valleys', 46.6, 10.5],
  ['brittany', 'Brittany coast', 'Headlands & Atlantic shores', 48.39, -4.49],
  ['finland', 'Finnish lakeland', 'Forests & inland waters', 61.5, 25.5],
];

export const paintings = [
  'royal-archive', 'scholars-globe', 'celestial-court', 'highland-host',
  'river-army', 'desert-banners', 'cavalry', 'mountain-castle',
];

const logo = '<img class="start-wordmark" src="/art/wordmark.webp" alt="AoeWorld" width="1200" height="327">';
const rule = '<div class="gold-rule" aria-hidden="true"><span>✦</span></div>';

export function views() {
  return `
    <section id="entry-intro" class="entry-screen" aria-label="Opening cinematic" hidden>
      <div class="intro-paintings" aria-hidden="true">${paintings.map((name, index) =>
        `<img src="/art/${name}.webp" alt="" class="intro-painting${index === 0 ? ' active' : ''}" width="1672" height="941">`).join('')}</div>
      <div class="intro-shade"></div>
      <div class="intro-caption"><span class="eyebrow">A WORLD AWAITS</span><p id="intro-line">Every story begins with a place.</p></div>
      <div class="intro-title">${logo}${rule}<p>A world to discover. A history to shape.</p></div>
      <button id="skip-cinematic" class="quiet-button" type="button">Skip intro <span>· any key or click</span></button>
    </section>
    <section id="main-menu" class="entry-screen" aria-label="Main menu" hidden>
      <div class="menu-rail">
        ${logo}<p class="eyebrow menu-subtitle">A WORLD TO DISCOVER</p>${rule}
        <nav aria-label="Game modes">
          <button id="single-player" class="gold-button primary" type="button"><span aria-hidden="true">⚔</span> Single Player <span aria-hidden="true">›</span></button>
          ${['Multiplayer', 'Campaign', 'Settings'].map(name => `<button class="gold-button" type="button" disabled>${name}<small>Coming later</small></button>`).join('')}
        </nav>
        ${rule}<p class="menu-note">Choose your colours.<br>Chart your corner of the world.</p>
      </div>
      <div class="menu-caption"><span class="eyebrow">THE ROYAL ARCHIVE</span><p>The world is yours to unfold.</p></div>
      <footer class="start-footer"><span>AOEWORLD</span><span>SINGLE PLAYER SKIRMISH</span></footer>
    </section>
    <section id="skirmish-setup" class="entry-screen" aria-labelledby="setup-title" hidden>
      <header class="setup-heading">${logo}<span class="eyebrow">SINGLE PLAYER · SKIRMISH</span><h1 id="setup-title">Game Setup</h1></header>
      <form id="skirmish-form" class="walnut-panel">
        <div class="setup-columns">
          <section class="colour-panel" aria-labelledby="colour-title"><span class="eyebrow">I · YOUR BANNER</span><h2 id="colour-title">Choose your colour</h2><p>The colour of your place in the world.</p>
            <div class="colour-grid" role="radiogroup" aria-label="Player colour">${colours.map(([name, hex], index) => `<label class="colour-option" style="--swatch:${hex}"><input type="radio" name="player-colour" value="${name}" aria-label="${name[0].toUpperCase() + name.slice(1)}" ${index === 0 ? 'checked' : ''}><span class="colour-swatch" aria-hidden="true">✦</span><span>${name[0].toUpperCase() + name.slice(1)}</span></label>`).join('')}</div>
            <div class="banner-preview"><span id="player-banner" aria-hidden="true">⚑</span><div><span class="eyebrow">YOUR COLOUR</span><strong id="colour-name">Blue</strong></div></div>
          </section>
          <section class="map-panel" aria-labelledby="map-title"><span class="eyebrow">II · YOUR WORLD</span><h2 id="map-title">Choose your map</h2>
            <div class="preset-grid">${presets.map(([id, name, caption], index) => `<button type="button" class="map-preset" data-preset="${id}" aria-label="${name}" aria-pressed="${index === 0}"><strong>${name}</strong><span>${caption}</span></button>`).join('')}</div>
            <div class="parchment-panel">
              <div class="map-heading"><strong id="chosen-location">Paris basin</strong><button type="button" id="custom-location" class="parchment-link">Custom location</button></div>
              <div class="coordinate-row"><label>Latitude<input id="start-latitude" aria-label="Starting latitude" type="number" min="-90" max="90" step="any" value="48.8566" required></label><label>Longitude<input id="start-longitude" aria-label="Starting longitude" type="number" min="-180" max="180" step="any" value="2.3522" required></label></div>
              <div class="coordinate-row"><label>Square size <span>km per side</span><input id="start-size" aria-label="Map size in kilometres" type="number" min="0.25" max="10000" step="any" value="30" required></label><label>Scale <span>compression to one</span><input id="start-scale" aria-label="Map compression" type="number" min="1" max="10000" step="0.001" value="30" required></label></div>
              <output id="skirmish-estimate" aria-live="polite">Estimating your landscape…</output>
            </div>
          </section>
        </div>
        <div class="setup-actions"><button id="setup-back" class="quiet-button" type="button">‹ Back to menu</button><p id="setup-status" role="status">Connecting to your world…</p><button id="start-game" class="gold-button primary" type="submit" disabled>Start Game <span aria-hidden="true">›</span></button></div>
      </form>
      <footer class="start-footer"><span>CHART A REAL PLACE. BEGIN YOUR STORY.</span><span>AOEWORLD</span></footer>
    </section>
    <section id="loading-screen" class="entry-screen" aria-labelledby="loading-title" hidden>
      <div class="loading-content">${logo}${rule}<span class="eyebrow">YOUR WORLD IS TAKING SHAPE</span><h1 id="loading-title">Preparing your landscape</h1>
        <p id="loading-location"></p><div id="loading-progress" role="progressbar" aria-label="Map preparation progress"><span></span></div>
        <p id="loading-phase" role="status" aria-live="polite">Submitting your map request…</p>
        <p id="loading-error" role="alert" hidden></p><button id="loading-back" class="gold-button" type="button" hidden>Back to setup</button>
      </div>
      <div class="loading-tip">${rule}<span class="eyebrow">FIELD NOTES</span><p id="loading-tip">Use the minimap to find your way across the landscape.</p></div>
    </section>`;
}
