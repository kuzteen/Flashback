<script lang="ts">
  import { onDestroy } from 'svelte';
  import SidePanel from './SidePanel.svelte';
  import ValueInput from './ValueInput.svelte';
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

  function startSlide(e: PointerEvent) {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLInputElement).setPointerCapture(e.pointerId);
    sliding = true;
    beginGesture();
  }

  function endSlide() {
    if (!sliding) return;
    sliding = false;
    endGesture();
  }

  onDestroy(endSlide);

  // Fila cuyo valor se está escribiendo a mano (clic derecho en el número o en el deslizador).
  let editing = $state<keyof Look | null>(null);

  function editValue(e: MouseEvent, key: keyof Look) {
    e.preventDefault();
    editing = key;
  }

  function typed(key: keyof Look, v: number | null) {
    editing = null;
    if (v !== null && v !== Math.round(look[key] * 100)) commit(withLook({ [key]: v / 100 }));
  }

  function reset(key: keyof Look) {
    if (look[key] !== 0) commit(withLook({ [key]: 0 }));
  }

  // Comparar solo tiene sentido con algo ajustado y el panel a la vista.
  $effect(() => {
    if (neutral || !ui.lookOpen) ui.compare = false;
  });

  function resetAll() {
    if (!neutral) commit({ ...editorState.edit, look: { ...NEUTRAL_LOOK } });
  }

</script>

<SidePanel side="left" open={ui.lookOpen} label={t('ed.look')} icon="wand" ontoggle={() => ui.toggleLook()}>
  <div class="body">
    <div class="top">
      <h3 class="title">{t('ed.look')}</h3>
      <button class="reset" disabled={neutral} onclick={resetAll}>{t('ed.lookReset')}</button>
    </div>
    {#each ROWS as r (r.key)}
      {@const v = Math.round(look[r.key] * 100)}
      {@const position = (v - r.min) / (100 - r.min)}
      {@const zero = -r.min / (100 - r.min)}
      <label class="row">
        <span class="rl">{t(r.label)}</span>
        <span class="val">
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <span class="rv mono" class:set={v !== 0} class:hidden={editing === r.key} oncontextmenu={(e) => editValue(e, r.key)}
            >{v}</span
          >
          {#if editing === r.key}
            <ValueInput value={v} min={r.min} max={100} label={t(r.label)} ondone={(n) => typed(r.key, n)} />
          {/if}
        </span>
        <input
          class="fader"
          class:signed={r.min < 0}
          style:--fill-from={Math.min(zero, position)}
          style:--fill-to={Math.max(zero, position)}
          type="range"
          min={r.min}
          max="100"
          step="1"
          value={v}
          onpointerdown={startSlide}
          onpointerup={endSlide}
          onpointercancel={endSlide}
          onlostpointercapture={endSlide}
          oninput={(e) => set(r.key, Number(e.currentTarget.value))}
          ondblclick={() => reset(r.key)}
          oncontextmenu={(e) => editValue(e, r.key)}
        />
      </label>
    {/each}
    <button class="compare" class:on={ui.compare} aria-pressed={ui.compare} disabled={neutral} onclick={() => (ui.compare = !ui.compare)}>
      {t('ed.compare')}
    </button>
    <p class="note">{t('ed.lookHint')}</p>
  </div>
</SidePanel>

<style>
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
  .val {
    position: relative;
    display: inline-flex;
    justify-content: flex-end;
  }
  .rv.hidden {
    visibility: hidden;
  }
  .row .fader {
    grid-column: 1 / -1;
  }
  .row .fader::-webkit-slider-runnable-track {
    background:
      linear-gradient(to right,
        var(--text-3) 0,
        var(--text-3) calc(12px + (100% - 24px) * var(--fill-to)),
        transparent calc(12px + (100% - 24px) * var(--fill-to))),
      var(--bg-3);
  }
  .row .fader.signed::-webkit-slider-runnable-track {
    background:
      linear-gradient(var(--text-3), var(--text-3)) center / 1px 6px no-repeat,
      linear-gradient(to right,
        transparent calc(12px + (100% - 24px) * var(--fill-from)),
        var(--text-3) calc(12px + (100% - 24px) * var(--fill-from)),
        var(--text-3) calc(12px + (100% - 24px) * var(--fill-to)),
        transparent calc(12px + (100% - 24px) * var(--fill-to))),
      var(--bg-3);
  }
  .compare {
    height: 32px;
    font-size: 12.5px;
    color: var(--text-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    transition: color 0.14s ease, background 0.14s ease, border-color 0.14s ease;
  }
  .compare:hover:not(:disabled) {
    color: var(--text-0);
    background: var(--bg-hover);
  }
  .compare.on {
    color: var(--text-0);
    border-color: var(--accent);
  }
  .compare:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .note {
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-3);
  }
</style>
