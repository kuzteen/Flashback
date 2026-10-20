<script lang="ts">
  import ValueInput from './ValueInput.svelte';
  import SidePanel from './SidePanel.svelte';
  import { pill } from '$lib/pill';
  import { beginGesture, commit, editorState, endGesture, preview } from '$lib/editor-state.svelte';
  import type { OutputFormat } from '$lib/edit-model';
  import { ZOOM_MAX } from '$lib/frame-math';
  import { t } from '$lib/i18n.svelte';
  import { ui } from './ui.svelte';

  const format = $derived(editorState.edit.format);

  // Cada cambio de formato es un paso de deshacer; elegir el que ya está no ensucia el historial.
  function set(next: OutputFormat) {
    if (JSON.stringify(next) === JSON.stringify(format)) return;
    commit({ ...editorState.edit, format: next });
  }

  const customZoom = $derived(format.kind === 'vertical' && format.fill === 'custom' ? (format.zoom ?? 0.5) : 0.5);

  // Arrastrar el deslizador es un gesto (un paso al soltar); con el teclado cada paso es uno.
  let sliding = false;

  function setZoom(z: number) {
    const next: OutputFormat = { kind: 'vertical', fill: 'custom', zoom: z };
    if (sliding) preview({ ...editorState.edit, format: next });
    else commit({ ...editorState.edit, format: next });
  }

  // Zoom escrito a mano (clic derecho en el porcentaje o en el deslizador).
  let editingZoom = $state(false);

  function editZoom(e: MouseEvent) {
    e.preventDefault();
    editingZoom = true;
  }

  function typedZoom(v: number | null) {
    editingZoom = false;
    if (v !== null && v !== Math.round(customZoom * 100)) setZoom(v / 100);
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

</script>

<SidePanel side="right" open={ui.formatOpen} label={t('ed.format')} icon="crop" ontoggle={() => ui.toggleFormat()}>
  <div class="body">
    <h3 class="title">{t('ed.format')}</h3>
    <div class="tiles" role="radiogroup" aria-label={t('ed.format')}>
      <button
        class="tile"
        role="radio"
        aria-checked={format.kind === 'horizontal'}
        class:on={format.kind === 'horizontal'}
        onclick={() => set({ kind: 'horizontal' })}
      >
        <span class="box"><span class="shape h"></span></span>
        <span class="name">{t('ed.fmtOriginal')}</span>
      </button>
      <button
        class="tile"
        role="radio"
        aria-checked={format.kind === 'vertical'}
        class:on={format.kind === 'vertical'}
        onclick={() => set({ kind: 'vertical', fill: format.kind === 'vertical' ? format.fill : 'crop' })}
      >
        <span class="box"><span class="shape v"></span></span>
        <span class="name">9:16</span>
      </button>
    </div>

    {#if format.kind === 'vertical'}
      <div class="seg" role="radiogroup" aria-label={t('ed.fill')} use:pill={{ key: format.fill }}>
        <button
          role="radio"
          aria-checked={format.fill === 'crop'}
          class:on={format.fill === 'crop'}
          onclick={() => set({ kind: 'vertical', fill: 'crop' })}
        >
          {t('ed.fillCrop')}
        </button>
        <button
          role="radio"
          aria-checked={format.fill === 'fit'}
          class:on={format.fill === 'fit'}
          onclick={() => set({ kind: 'vertical', fill: 'fit' })}
        >
          {t('ed.fillFit')}
        </button>
        <button
          role="radio"
          aria-checked={format.fill === 'custom'}
          class:on={format.fill === 'custom'}
          onclick={() => set({ kind: 'vertical', fill: 'custom', zoom: customZoom })}
        >
          {t('ed.fillCustom')}
        </button>
      </div>
      {#if format.fill === 'custom'}
        <label class="zoom">
          <span class="zl">{t('ed.zoom')}</span>
          <input
            class="fader"
            type="range"
            min="0"
            max={ZOOM_MAX * 100}
            step="1"
            value={Math.round(customZoom * 100)}
            onpointerdown={startSlide}
            onpointerup={endSlide}
            onpointercancel={endSlide}
            oninput={(e) => setZoom(Number(e.currentTarget.value) / 100)}
            oncontextmenu={editZoom}
          />
          <span class="val">
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span class="zv mono" class:hidden={editingZoom} oncontextmenu={editZoom}>{Math.round(customZoom * 100)}%</span>
            {#if editingZoom}
              <ValueInput value={Math.round(customZoom * 100)} min={0} max={ZOOM_MAX * 100} label={t('ed.zoom')} ondone={typedZoom} />
            {/if}
          </span>
        </label>
      {/if}
      <p class="note">{t('ed.frameHint')}</p>
    {/if}
  </div>
</SidePanel>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 230px;
  }
  .title {
    margin-bottom: 4px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  /* Dos casillas lado a lado: la silueta de la proporción se entiende antes que cualquier texto. */
  .tiles {
    display: flex;
    gap: 12px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 7px;
  }
  .box {
    width: 64px;
    height: 64px;
    display: grid;
    place-items: center;
    background: var(--surface);
    border: 2px solid var(--line);
    border-radius: var(--r-md);
    transition: border-color 0.14s ease, background 0.14s ease;
  }
  .tile:hover .box {
    background: var(--bg-2);
  }
  .tile.on .box {
    border-color: var(--accent);
    background: var(--bg-2);
  }
  .shape {
    border: 2px solid var(--text-2);
    border-radius: 3px;
  }
  .tile.on .shape {
    border-color: var(--text-0);
  }
  .shape.h {
    width: 30px;
    height: 18px;
  }
  .shape.v {
    width: 18px;
    height: 30px;
  }
  .name {
    font-size: 12.5px;
    color: var(--text-2);
  }
  .tile.on .name {
    color: var(--text-0);
  }
  .seg {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 2px;
    padding: 3px;
    margin-top: 4px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
  }
  .seg button {
    height: 30px;
    font-size: 12.5px;
    color: var(--text-2);
    border-radius: 4px;
    transition: color 0.14s ease, background 0.14s ease;
  }
  .seg button:hover {
    color: var(--text-0);
  }
  .seg button.on {
    color: var(--text-0);
  }
  .seg > :global(.slide-pill) {
    background: var(--bg-3);
    border-radius: 4px;
  }
  .zoom {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 10px;
    margin-top: 2px;
  }
  .zl {
    font-size: 12px;
    color: var(--text-2);
  }
  .zv {
    min-width: 34px;
    font-size: 11.5px;
    text-align: right;
    color: var(--text-1);
  }
  .val {
    position: relative;
    display: inline-flex;
    justify-content: flex-end;
  }
  .zv.hidden {
    visibility: hidden;
  }
  .note {
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-3);
  }
</style>
