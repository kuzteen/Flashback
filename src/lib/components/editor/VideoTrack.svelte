<script lang="ts">
  import FilmTiles from './FilmTiles.svelte';
  import { beginGesture, editorState, endGesture, preview } from '$lib/editor-state.svelte';
  import { moveTo, segLen, snap, snapTargets, sortByPos, trimToPos } from '$lib/edit-model';
  import { SNAP_PX, extentMs, outStartOf, posToOut } from '$lib/timeline-math';
  import { t } from '$lib/i18n.svelte';
  import { playback } from './playback.svelte';
  import { ui } from './ui.svelte';
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

  // Mantener pulsado un bloque sin moverlo lo levanta para moverlo; si el puntero se mueve antes,
  // el gesto se queda en buscar.
  const LIFT_MS = 300;
  const LIFT_CANCEL_PX = 6;

  type Gesture =
    | { kind: 'scrub' }
    | { kind: 'pending'; index: number; downX: number; timer: ReturnType<typeof setTimeout> }
    | { kind: 'lift'; index: number; grab: number }
    | { kind: 'trim'; index: number; edge: 'start' | 'end'; grab: number };

  let g: Gesture | null = null;
  let lane = $state<HTMLDivElement | null>(null);
  let lifted = $state<number | null>(null);
  let trimming = $state(false);

  const segs = $derived(editorState.edit.segments);
  const px = (ms: number) => (mpp > 0 ? ms / mpp : 0);
  const snapMs = (e: PointerEvent) => (e.altKey ? 0 : SNAP_PX * mpp);

  function seekAt(clientX: number) {
    playback.seekOutput(posToOut(editorState.edit.segments, posAt(clientX)));
  }

  // La captura va al carril y no al bloque: así los movimientos llegan al mismo manejador pase
  // lo que pase por debajo del puntero.
  function capture(e: PointerEvent) {
    lane?.setPointerCapture(e.pointerId);
  }

  function onLaneDown(e: PointerEvent) {
    if (e.button !== 0) return;
    capture(e);
    playback.pauseForSeek();
    g = { kind: 'scrub' };
    seekAt(e.clientX);
  }

  function onBlockDown(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    e.stopPropagation();
    capture(e);
    playback.pauseForSeek();
    editorState.active = i;
    seekAt(e.clientX);
    if (segs.length < 2) {
      g = { kind: 'scrub' };
      return;
    }
    const downX = e.clientX;
    const timer = setTimeout(() => {
      if (g?.kind !== 'pending') return;
      const s = editorState.edit.segments[i];
      if (!s) return;
      beginGesture();
      g = { kind: 'lift', index: i, grab: posAt(downX) - s.posMs };
      lifted = i;
    }, LIFT_MS);
    g = { kind: 'pending', index: i, downX, timer };
  }

  function onGripDown(e: PointerEvent, i: number, edge: 'start' | 'end') {
    if (e.button !== 0) return;
    e.stopPropagation();
    e.preventDefault();
    capture(e);
    playback.pauseForSeek();
    editorState.active = i;
    const s = segs[i];
    const edgePos = edge === 'start' ? s.posMs : s.posMs + segLen(s);
    beginGesture();
    g = { kind: 'trim', index: i, edge, grab: posAt(e.clientX) - edgePos };
    trimming = true;
  }

  function onMove(e: PointerEvent) {
    if (!g) return;
    if (g.kind === 'scrub') {
      seekAt(e.clientX);
    } else if (g.kind === 'pending') {
      if (Math.abs(e.clientX - g.downX) > LIFT_CANCEL_PX) {
        clearTimeout(g.timer);
        g = { kind: 'scrub' };
      }
      seekAt(e.clientX);
    } else if (g.kind === 'lift') {
      const cur = editorState.edit;
      const extent = extentMs(editorState.durationMs, ui.zoom);
      const r = moveTo(cur.segments, g.index, posAt(e.clientX) - g.grab, extent, snapMs(e), [headPos]);
      preview({ ...cur, segments: r.segments });
      ui.guideAt = r.snappedAt;
    } else {
      const cur = editorState.edit;
      const want = posAt(e.clientX) - g.grab;
      const sn = snap(want, snapTargets(cur.segments, g.index, headPos), snapMs(e));
      const next = trimToPos(cur.segments, g.index, g.edge, sn.value);
      preview({ ...cur, segments: next });
      const s = next[g.index];
      const edgePos = g.edge === 'start' ? s.posMs : s.posMs + segLen(s);
      // La guía solo si el borde llegó de verdad: los límites del bloque pueden impedirlo.
      ui.guideAt = sn.at !== null && Math.abs(edgePos - sn.at) < 0.5 ? sn.at : null;
      const start = outStartOf(next, g.index);
      if (g.edge === 'start') playback.peek(start, s.startMs);
      else playback.peek(start + segLen(s), s.endMs);
    }
  }

  function onUp() {
    const cur = g;
    g = null;
    if (!cur) return;
    if (cur.kind === 'pending') clearTimeout(cur.timer);
    if (cur.kind === 'lift') {
      // Al soltar se ordena por posición (izquierda a derecha = orden de salida) y se vuelve a
      // seleccionar el bloque movido por su posición, única porque nunca se solapan.
      const pos = editorState.edit.segments[cur.index]?.posMs ?? 0;
      preview({ ...editorState.edit, segments: sortByPos(editorState.edit.segments) });
      const idx = editorState.edit.segments.findIndex((s) => s.posMs === pos);
      if (idx >= 0) editorState.active = idx;
      endGesture();
      playback.settle();
    } else if (cur.kind === 'trim') {
      endGesture();
    }
    lifted = null;
    trimming = false;
    ui.guideAt = null;
    playback.resumeAfterGesture();
  }

  function onContext(e: MouseEvent, i: number) {
    e.preventDefault();
    e.stopPropagation();
    editorState.active = i;
    ui.blockMenu = {
      x: Math.min(e.clientX, window.innerWidth - 190),
      y: Math.min(e.clientY, window.innerHeight - 100),
      index: i,
    };
  }

  function menuToggle() {
    const i = ui.blockMenu?.index;
    ui.blockMenu = null;
    if (i != null) toggleBlock(i);
  }

  function menuRemove() {
    const i = ui.blockMenu?.index;
    ui.blockMenu = null;
    if (i != null) removeBlock(i);
  }
</script>

<div class="row">
  <div class="head"></div>
  <div
    class="lane"
    class:trimming
    class:lifting={lifted !== null}
    bind:this={lane}
    style:width="{width}px"
    role="presentation"
    onpointerdown={onLaneDown}
    onpointermove={onMove}
    onpointerup={onUp}
    onpointercancel={onUp}
  >
    {#each segs as s, i (i)}
      <div
        class="block"
        class:active={i === editorState.active}
        class:disabled={s.disabled}
        class:lifted={lifted === i}
        style:left="{px(s.posMs)}px"
        style:width="{px(segLen(s))}px"
        role="presentation"
        onpointerdown={(e) => onBlockDown(e, i)}
        oncontextmenu={(e) => onContext(e, i)}
      >
        <div class="film">
          <FilmTiles startMs={s.startMs} left={px(s.posMs)} width={px(segLen(s))} {mpp} {viewX} {viewW} />
        </div>
        {#if i === editorState.active}
          <span class="grip start" role="presentation" onpointerdown={(e) => onGripDown(e, i, 'start')}></span>
          <span class="grip end" role="presentation" onpointerdown={(e) => onGripDown(e, i, 'end')}></span>
        {/if}
      </div>
    {/each}
  </div>
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
  <div class="ctx" role="menu" style:left="{ui.blockMenu.x}px" style:top="{ui.blockMenu.y}px">
    <button role="menuitem" onclick={menuToggle}>
      {segs[ui.blockMenu.index]?.disabled ? t('ed.enable') : t('ed.disable')}
    </button>
    <button role="menuitem" class="danger" disabled={segs.length <= 1} onclick={menuRemove}>
      {t('ed.deleteBlock')}
    </button>
  </div>
{/if}

<style>
  .row {
    display: flex;
    height: 58px;
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
  .lane {
    position: relative;
    flex: none;
    margin-left: var(--pad);
    clip-path: var(--lane-clip);
    cursor: pointer;
    touch-action: none;
  }
  .block {
    position: absolute;
    top: 9px;
    bottom: 9px;
    border-radius: 6px;
    background: linear-gradient(to bottom, var(--bg-3), var(--bg-2));
    transition: left 0.18s ease, box-shadow 0.14s ease, transform 0.14s ease;
  }
  /* El borde va por encima de la tira de fotogramas, que lo taparía si fuera del propio bloque. */
  .block::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    box-shadow: inset 0 0 0 1px var(--line-strong);
    transition: box-shadow 0.14s ease;
    pointer-events: none;
  }
  .film {
    position: absolute;
    inset: 0;
    border-radius: inherit;
    overflow: hidden;
  }
  .block.disabled .film {
    opacity: 0.3;
  }
  .lane.trimming .block,
  .lane.lifting .block {
    transition: box-shadow 0.14s ease, transform 0.14s ease;
  }
  .block.active::after {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .block.disabled {
    background: repeating-linear-gradient(-45deg, var(--bg-hover) 0 6px, transparent 6px 12px);
  }
  .block.lifted {
    z-index: 2;
    transform: scale(1.05);
    box-shadow: 0 12px 26px -8px rgba(0, 0, 0, 0.8);
    cursor: grabbing;
  }
  .grip {
    position: absolute;
    z-index: 1;
    top: -2px;
    bottom: -2px;
    width: 10px;
    border-radius: 4px;
    background: var(--accent);
    cursor: ew-resize;
  }
  .grip.start {
    left: -5px;
  }
  .grip.end {
    right: -5px;
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
    background: var(--bg-3);
    color: var(--text-0);
  }
  .ctx .danger {
    color: var(--rec);
  }
  .ctx .danger:hover {
    background: color-mix(in srgb, var(--rec) 12%, transparent);
    color: var(--rec);
  }
  .ctx button:disabled {
    opacity: 0.4;
  }
</style>
