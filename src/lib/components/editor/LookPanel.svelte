<script lang="ts">
  import Icon from '../Icon.svelte';
  import { beginGesture, commit, editorState, endGesture, preview } from '$lib/editor-state.svelte';
  import { NEUTRAL_LOOK, isNeutral, type Look } from '$lib/look';
  import { t } from '$lib/i18n.svelte';
  import { ui } from './ui.svelte';

  const look = $derived(editorState.edit.look);
  const neutral = $derived(isNeutral(look));

  const ROWS: { key: keyof Look; label: string; min: number }[] = [
    { key: 'brightness', label: 'ed.brightness', min: -100 },
    { key: 'contrast', label: 'ed.contrast', min: -100 },
    { key: 'saturation', label: 'ed.saturation', min: -100 },
    { key: 'temperature', label: 'ed.temperature', min: -100 },
    { key: 'sharpness', label: 'ed.sharpness', min: 0 },
  ];


  const withLook = (patch: Partial<Look>) => ({ ...editorState.edit, look: { ...look, ...patch } });

  // Como el zoom del formato: arrastrar es un gesto (un paso al soltar) y el teclado da un paso
  // por pulsación.
  let sliding = false;

  function set(key: keyof Look, v: number) {
    const next = withLook({ [key]: v / 100 });
    if (sliding) preview(next);
    else commit(next);
  }

  function startSlide() {
    sliding = true;
    beginGesture();
  }

  function endSlide() {
    if (!sliding) return;
    sliding = false;
    endGesture();
  }

  function reset(key: keyof Look) {
    if (look[key] !== 0) commit(withLook({ [key]: 0 }));
  }

  function resetAll() {
    if (!neutral) commit({ ...editorState.edit, look: { ...NEUTRAL_LOOK } });
  }
</script>

<aside class="panel" class:open={ui.lookOpen}>
  <button
    class="toggle"
    class:on={ui.lookOpen}
    aria-label={t('ed.look')}
    aria-expanded={ui.lookOpen}
    data-tip={t('ed.look')}
    data-tip-pos="below"
    data-tip-align="start"
    onclick={() => ui.toggleLook()}
  >
    <Icon name="wand" size={18} />
  </button>

  {#if ui.lookOpen}
    <div class="body">
      <div class="top">
        <h3 class="title">{t('ed.look')}</h3>
        <button class="reset" disabled={neutral} onclick={resetAll}>{t('ed.lookReset')}</button>
      </div>
      {#each ROWS as r (r.key)}
        {@const v = Math.round(look[r.key] * 100)}
        <label class="row">
          <span class="rl">{t(r.label)}</span>
          <span class="rv mono" class:set={v !== 0}>{v}</span>
          <input
            class="fader"
            type="range"
            min={r.min}
            max="100"
            step="1"
            value={v}
            onpointerdown={startSlide}
            onpointerup={endSlide}
            onpointercancel={endSlide}
            oninput={(e) => set(r.key, Number(e.currentTarget.value))}
            ondblclick={() => reset(r.key)}
          />
        </label>
      {/each}
      <p class="note">{t('ed.lookHint')}</p>
    </div>
  {/if}
</aside>

<style>
  /* Gemelo del panel de formato, en el lado izquierdo: plegado es solo el botón sobre el visor;
     abierto, una columna. */
  .panel {
    position: absolute;
    top: 12px;
    left: 12px;
    z-index: 5;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 14px;
  }
  .panel.open {
    position: static;
    flex: none;
    width: var(--format-w);
    padding: 10px 14px 14px;
    background: var(--bg-0);
    border-right: 1px solid var(--line);
    overflow: hidden;
  }
  .toggle {
    align-self: flex-start;
    width: 36px;
    height: 36px;
    flex: none;
    display: grid;
    place-items: center;
    color: var(--text-2);
    border-radius: var(--r-sm);
    transition: color 0.14s ease, background 0.14s ease;
  }
  .toggle:hover,
  .toggle.on {
    color: var(--text-0);
    background: var(--bg-hover);
  }
  .panel:not(.open) .toggle {
    color: var(--text-1);
    background: var(--glass);
    backdrop-filter: blur(12px);
    border: 1px solid var(--line);
  }
  .panel:not(.open) .toggle:hover {
    color: var(--text-0);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 230px;
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .title {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .reset {
    font-size: 12px;
    color: var(--text-2);
    transition: color 0.14s ease;
  }
  .reset:hover:not(:disabled) {
    color: var(--text-0);
  }
  .reset:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    row-gap: 6px;
  }
  .rl {
    font-size: 12.5px;
    color: var(--text-1);
  }
  .rv {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .rv.set {
    color: var(--text-0);
  }
  .row .fader {
    grid-column: 1 / -1;
  }
  .note {
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-3);
  }
</style>
