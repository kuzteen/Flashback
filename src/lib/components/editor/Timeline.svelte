<script lang="ts">
  import { tick } from 'svelte';
  import { editorState } from '$lib/editor-state.svelte';
  import {
    contentWidth,
    extentMs,
    formatRulerLabel,
    msPerPx,
    outToPos,
    posToOut,
    rulerStep,
    rulerTicks,
    zoomBy,
  } from '$lib/timeline-math';
  import { playback } from './playback.svelte';
  import { ui } from './ui.svelte';
  import VideoTrack from './VideoTrack.svelte';
  import AudioTrack from './AudioTrack.svelte';

  const GUTTER = 208;

  let scrollEl = $state<HTMLDivElement | null>(null);
  let viewW = $state(0);
  let scrollLeft = $state(0);

  const total = $derived(editorState.durationMs);
  const segs = $derived(editorState.edit.segments);
  const mpp = $derived(msPerPx(total, viewW, ui.zoom));
  const width = $derived(contentWidth(viewW, ui.zoom));
  const extent = $derived(extentMs(total, ui.zoom));
  const step = $derived(mpp > 0 ? rulerStep(mpp) : 1000);
  // Solo las marcas visibles (más un margen): con zoom alto la regla entera serían miles.
  const ticks = $derived.by(() => {
    const from = (scrollLeft - 100) * mpp;
    const to = (scrollLeft + viewW + 100) * mpp;
    return rulerTicks(extent, mpp).filter((tk) => tk.ms >= from && tk.ms <= to);
  });
  const headPos = $derived(outToPos(segs, playback.outPos));
  const px = (ms: number) => (mpp > 0 ? ms / mpp : 0);

  $effect(() => {
    const el = scrollEl;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      viewW = Math.max(0, el.clientWidth - GUTTER);
    });
    ro.observe(el);
    return () => ro.disconnect();
  });

  // Listener no pasivo: Ctrl+rueda tiene que poder cancelar el scroll para hacer zoom. El punto
  // bajo el puntero se queda quieto al ampliar.
  $effect(() => {
    const el = scrollEl;
    if (!el) return;
    const onWheel = async (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      const r = el.getBoundingClientRect();
      const offset = e.clientX - r.left - GUTTER;
      const anchor = posAt(e.clientX);
      const z = zoomBy(ui.zoom, e.deltaY);
      ui.zoom = z;
      await tick();
      const m = msPerPx(total, viewW, z);
      if (m > 0) el.scrollLeft = Math.max(0, anchor / m - offset);
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  });

  function posAt(clientX: number): number {
    if (!scrollEl || mpp <= 0) return 0;
    const r = scrollEl.getBoundingClientRect();
    return Math.max(0, Math.min(extent, (clientX - r.left - GUTTER + scrollEl.scrollLeft) * mpp));
  }

  // Regla: un clic mueve el cabezal; arrastrar marca un rango.
  let rulerDown: { x: number; pos: number; ranging: boolean } | null = null;

  function onRulerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    rulerDown = { x: e.clientX, pos: posAt(e.clientX), ranging: false };
  }

  function onRulerMove(e: PointerEvent) {
    const d = rulerDown;
    if (!d) return;
    if (!d.ranging && Math.abs(e.clientX - d.x) < 4) return;
    d.ranging = true;
    const p = posAt(e.clientX);
    ui.range = { from: Math.min(p, d.pos), to: Math.max(p, d.pos) };
  }

  function onRulerUp() {
    const d = rulerDown;
    rulerDown = null;
    if (!d || d.ranging) return;
    ui.range = null;
    playback.seekOutput(posToOut(segs, d.pos));
  }

  let knobDrag = false;

  function onKnobDown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    playback.pauseForSeek();
    knobDrag = true;
  }

  function onKnobMove(e: PointerEvent) {
    if (knobDrag) playback.seekOutput(posToOut(segs, posAt(e.clientX)));
  }

  function onKnobUp() {
    if (!knobDrag) return;
    knobDrag = false;
    playback.resumeAfterGesture();
  }
</script>

<div class="tl" bind:this={scrollEl} style:--gutter="{GUTTER}px" onscroll={(e) => (scrollLeft = e.currentTarget.scrollLeft)}>
  <div class="inner" style:width="{GUTTER + width}px">
    <div class="row ruler-row">
      <div class="head"></div>
      <div
        class="ruler"
        style:width="{width}px"
        role="presentation"
        onpointerdown={onRulerDown}
        onpointermove={onRulerMove}
        onpointerup={onRulerUp}
        onpointercancel={onRulerUp}
      >
        {#each ticks as tk (tk.ms)}
          <span class="tick" class:major={tk.major} style:left="{px(tk.ms)}px">
            {#if tk.major}<span class="lbl mono">{formatRulerLabel(tk.ms, step)}</span>{/if}
          </span>
        {/each}
      </div>
    </div>

    <VideoTrack {mpp} {width} {posAt} {headPos} />
    <AudioTrack kind="sys" {mpp} {width} {viewW} {scrollLeft} />
    {#if editorState.loading || editorState.mic}
      <AudioTrack kind="mic" {mpp} {width} {viewW} {scrollLeft} />
    {/if}

    {#if ui.range}
      <div class="range" style:left="{GUTTER + px(ui.range.from)}px" style:width="{px(ui.range.to - ui.range.from)}px"></div>
    {/if}
    {#if ui.guideAt !== null}
      <div class="guide" style:left="{GUTTER + px(ui.guideAt)}px"></div>
    {/if}
    <div class="playhead" style:left="{GUTTER + px(headPos)}px">
      <span
        class="knob"
        role="presentation"
        onpointerdown={onKnobDown}
        onpointermove={onKnobMove}
        onpointerup={onKnobUp}
        onpointercancel={onKnobUp}
      ></span>
    </div>
  </div>
</div>

<style>
  .tl {
    flex: 1;
    min-height: 0;
    overflow-x: auto;
    overflow-y: auto;
    background: var(--base);
  }
  .inner {
    position: relative;
    min-height: 100%;
  }
  .row {
    display: flex;
  }
  .ruler-row {
    height: 28px;
    border-bottom: 1px solid var(--line);
  }
  .head {
    position: sticky;
    left: 0;
    z-index: 3;
    flex: none;
    width: var(--gutter);
    background: var(--bg-0);
    border-right: 1px solid var(--line);
  }
  .ruler {
    position: relative;
    flex: none;
    cursor: text;
    touch-action: none;
  }
  .tick {
    position: absolute;
    bottom: 0;
    width: 1px;
    height: 5px;
    background: var(--line-strong);
  }
  .tick.major {
    height: 10px;
    background: var(--text-3);
  }
  .lbl {
    position: absolute;
    left: 5px;
    bottom: 9px;
    font-size: 10.5px;
    color: var(--text-3);
    white-space: nowrap;
    pointer-events: none;
  }
  .range {
    position: absolute;
    top: 0;
    bottom: 0;
    z-index: 1;
    background: rgba(255, 255, 255, 0.08);
    box-shadow:
      inset 1px 0 0 var(--accent),
      inset -1px 0 0 var(--accent);
    pointer-events: none;
  }
  .guide {
    position: absolute;
    top: 0;
    bottom: 0;
    z-index: 2;
    width: 1px;
    margin-left: -0.5px;
    background: var(--gold);
    pointer-events: none;
  }
  .playhead {
    position: absolute;
    top: 0;
    bottom: 0;
    z-index: 2;
    width: 2px;
    margin-left: -1px;
    background: var(--accent);
    pointer-events: none;
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 50%;
    width: 13px;
    height: 13px;
    border-radius: 3px 3px 7px 7px;
    background: var(--accent);
    transform: translateX(-50%);
    pointer-events: auto;
    cursor: grab;
    touch-action: none;
  }
</style>
