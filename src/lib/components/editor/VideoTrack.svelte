<script lang="ts">
  import { hoverPill } from '$lib/pill';
  import Icon from '../Icon.svelte';
  import BlockLane from './BlockLane.svelte';
  import FilmTiles from './FilmTiles.svelte';
  import { editorState } from '$lib/editor-state.svelte';
  import { segLen } from '$lib/edit-model';
  import { t } from '$lib/i18n.svelte';
  import { GRIP_COLORS, ui } from './ui.svelte';
  import { removeBlock, toggleBlock } from './actions';

  let {
    mpp,
    width,
    viewX,
    viewW,
    posAt,
    headPos,
  }: {
    mpp: number;
    width: number;
    viewX: number;
    viewW: number;
    posAt: (clientX: number) => number;
    headPos: number;
  } = $props();

  const segs = $derived(editorState.edit.segments);
  const px = (ms: number) => (mpp > 0 ? ms / mpp : 0);

  function menuToggle() {
    const i = ui.blockMenu?.index;
    ui.blockMenu = null;
    if (i != null) toggleBlock(i);
  }

  // El menú se queda abierto al elegir: así se prueban colores viéndolos en la pista.
  function menuColor(index: number) {
    const m = ui.blockMenu;
    if (m) ui.setGrip(m.track, index);
  }

  // El submenú de colores se abre al lado del menú y a la altura de su fila; si no cabe a la
  // derecha se vuelca a la izquierda, y en vertical se sube lo justo para entrar en pantalla.
  const EDGE = 8;
  let colorOpen = $state(false);
  let ctxEl = $state<HTMLElement | null>(null);
  let colorItem = $state<HTMLElement | null>(null);
  let colorMenu = $state<HTMLElement | null>(null);

  $effect(() => {
    void ui.blockMenu;
    colorOpen = false;
  });

  $effect(() => {
    const el = colorMenu;
    const item = colorItem;
    const panel = ctxEl;
    if (!el || !item || !panel) return;
    const p = panel.getBoundingClientRect();
    const i = item.getBoundingClientRect();
    const r = el.getBoundingClientRect();
    let x = p.right + 2;
    if (x + r.width + EDGE > window.innerWidth) x = Math.max(EDGE, p.left - 2 - r.width);
    const y = Math.max(EDGE, Math.min(i.top - 5, window.innerHeight - r.height - EDGE));
    el.style.left = `${x}px`;
    el.style.top = `${y}px`;
    el.style.visibility = 'visible';
  });

  function menuRemove() {
    const i = ui.blockMenu?.index;
    ui.blockMenu = null;
    if (i != null) removeBlock(i);
  }
</script>

<div class="row">
  <div class="head"></div>
  <BlockLane track="video" {mpp} {width} {posAt} {headPos}>
    {#snippet block(s)}
      <FilmTiles startMs={s.startMs} left={px(s.posMs)} width={px(segLen(s))} {mpp} {viewX} {viewW} />
    {/snippet}
  </BlockLane>
</div>

{#if ui.blockMenu}
  <div
    class="ctx-backdrop"
    role="presentation"
    onpointerdown={() => (ui.blockMenu = null)}
    oncontextmenu={(e) => {
      e.preventDefault();
      ui.blockMenu = null;
    }}
  ></div>
  <div
    class="ctx"
    role="menu"
    bind:this={ctxEl}
    style:left="{ui.blockMenu.x}px"
    style:top="{ui.blockMenu.y}px"
    use:hoverPill={{ axis: 'y', selector: '[role="menuitem"]' }}
  >
    <button
      class="sub"
      class:open={colorOpen}
      role="menuitem"
      aria-haspopup="menu"
      aria-expanded={colorOpen}
      bind:this={colorItem}
      onmouseenter={() => (colorOpen = true)}
      onclick={() => (colorOpen = !colorOpen)}
    >
      <Icon name="eyedropper" size={16} />
      {t('ed.gripColor')}
      <Icon name="chevron-right" size={13} sw={2.2} />
    </button>
    <div class="ctx-sep"></div>
    <button role="menuitem" onmouseenter={() => (colorOpen = false)} onclick={menuToggle}>
      <Icon name={segs[ui.blockMenu.index]?.disabled ? 'clips-fill' : 'clips-off'} size={16} />
      {segs[ui.blockMenu.index]?.disabled ? t('ed.enable') : t('ed.disable')}
    </button>
    <button
      role="menuitem"
      class="danger"
      data-tone="danger"
      disabled={segs.length <= 1}
      onmouseenter={() => (colorOpen = false)}
      onclick={menuRemove}
    >
      <Icon name="trash" size={16} sw={2} />
      {t('ed.deleteBlock')}
    </button>
  </div>
  {#if colorOpen}
    {@const track = ui.blockMenu.track}
    <div class="ctx colors" role="menu" bind:this={colorMenu} use:hoverPill={{ axis: 'y' }}>
      {#each GRIP_COLORS as color, n (n)}
        <button role="menuitemradio" aria-checked={ui.grips[track] === n} onclick={() => menuColor(n)}>
          <span class="chip" style:--c={color.value}></span>
          <span class="nm">{t(color.name)}</span>
          {#if ui.grips[track] === n}<Icon name="check" size={14} sw={2.4} />{/if}
        </button>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .row {
    display: flex;
    height: 59px;
    border-bottom: 1px solid var(--line);
  }
  .head {
    position: sticky;
    left: 0;
    z-index: 3;
    flex: none;
    width: var(--gutter);
    background: var(--base);
    border-right: 1px solid var(--line);
  }
  .ctx-backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
  }
  .ctx {
    position: fixed;
    z-index: 201;
    min-width: 170px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
  }
  .ctx button {
    padding: 8px 10px;
    font-size: 13px;
    text-align: left;
    color: var(--text-1);
    border-radius: 6px;
  }
  .ctx button:hover {
    color: var(--text-0);
  }
  .ctx > :global(.slide-pill) {
    background: var(--bg-3);
    border-radius: 6px;
  }
  .ctx > :global(.slide-pill[data-tone='danger']) {
    background: color-mix(in srgb, var(--rec) 12%, transparent);
  }
  .ctx .danger {
    color: var(--rec);
  }
  .ctx .danger:hover {
    color: var(--rec);
  }
  .ctx button:disabled {
    opacity: 0.4;
  }
  .ctx button {
    display: flex;
    align-items: center;
    gap: 10px;
    white-space: nowrap;
  }
  .ctx button :global(svg) {
    flex: none;
  }
  /* La flecha del submenú empuja a la derecha y mantiene el color apagado. */
  .sub > :global(svg:last-child) {
    margin-left: auto;
    color: var(--text-3);
  }
  .ctx .sub.open {
    color: var(--text-0);
    background: var(--bg-3);
  }
  .colors {
    z-index: 202;
    left: 0;
    top: 0;
    visibility: hidden;
    min-width: 0;
    width: max-content;
  }
  .chip {
    flex: none;
    width: 14px;
    height: 14px;
    border-radius: 4px;
    background: var(--c);
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.25);
  }
  .colors .nm {
    flex: 1;
    min-width: 72px;
  }
  .colors button > :global(svg) {
    color: var(--text-0);
  }
  .ctx-sep {
    height: 1px;
    margin: 2px 4px 4px;
    background: var(--line);
  }
</style>
