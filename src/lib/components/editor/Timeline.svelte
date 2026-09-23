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
  // Aire a ambos lados de la línea de tiempo para que no quede pegada a la cabecera de pistas
  // ni al borde de la ventana. El tiempo 0 empieza en GUTTER + PAD.
  const PAD = 16;
  const ORIGIN = GUTTER + PAD;
  // A la derecha el hueco reservado de la barra vertical (11 px, ver app.css) ya forma parte del
  // margen: el carril solo añade lo que falta hasta PAD.
  const PAD_R = PAD - 11;

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
  // Ventana visible en coordenadas de carril: siempre la misma franja que ocupa la línea de tiempo
  // sin zoom. Lo que queda fuera se recorta, así que con zoom nada asoma por debajo de la cabecera
  // de pistas ni por los márgenes. En los extremos del contenido se deja holgura para los tiradores
  // y el cabezal, que sobresalen unos píxeles del bloque.
  const SLACK = 12;
  const clipL = $derived(scrollLeft > 0.5 ? scrollLeft : -SLACK);
  const clipR = $derived(scrollLeft + viewW >= width - 0.5 ? width + SLACK : scrollLeft + viewW);
  const laneClip = $derived(
    `polygon(${clipL}px -40px, ${clipR}px -40px, ${clipR}px calc(100% + 40px), ${clipL}px calc(100% + 40px))`,
  );

  $effect(() => {
    const el = scrollEl;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      viewW = Math.max(0, el.clientWidth - GUTTER - PAD - PAD_R);
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
      const offset = e.clientX - r.left - ORIGIN;
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
    return Math.max(0, Math.min(extent, (clientX - r.left - ORIGIN + scrollEl.scrollLeft) * mpp));
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

<div class="tl" bind:this={scrollEl} style:--gutter="{GUTTER}px" style:--pad="{PAD}px" style:--lane-clip={laneClip} onscroll={(e) => (scrollLeft = e.currentTarget.scrollLeft)}>
  <div class="inner" style:width="{ORIGIN + width + PAD_R}px">
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

    <VideoTrack {mpp} {width} viewX={scrollLeft} {viewW} {posAt} {headPos} />
    <AudioTrack kind="sys" {mpp} {width} viewX={scrollLeft} {viewW} />
    {#if editorState.loading || editorState.mic}
      <AudioTrack kind="mic" {mpp} {width} viewX={scrollLeft} {viewW} />
    {/if}

    <div class="over" style:left="{ORIGIN}px" style:width="{width}px">
      {#if ui.range}
        <div class="range" style:left="{px(ui.range.from)}px" style:width="{px(ui.range.to - ui.range.from)}px"></div>
      {/if}
      {#if ui.guideAt !== null}
        <div class="guide" style:left="{px(ui.guideAt)}px"></div>
      {/if}
      <div class="playhead" style:left="{px(headPos)}px">
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
</div>

<style>
  .tl {
    flex: 1;
    min-height: 0;
    overflow-x: auto;
    overflow-y: auto;
    /* El hueco de la barra vertical está siempre reservado: si apareciera solo al encoger el panel,
       el ancho útil cambiaría y toda la línea de tiempo se reescalaría de golpe. */
    scrollbar-gutter: stable;
    background: var(--base);
  }
  .inner {
    position: relative;
    min-height: 100%;
  }
  .row {
    display: flex;
  }
  /* La línea bajo la regla va solo en el carril: en la columna de cabeceras la regla y el vídeo se
     leen como un bloque. */
  .ruler-row {
    position: relative;
    height: 28px;
  }
  .ruler-row::after {
    content: '';
    position: absolute;
    left: var(--gutter);
    right: 0;
    bottom: 0;
    height: 1px;
    background: var(--line);
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
  .ruler {
    position: relative;
    flex: none;
    margin-left: var(--pad);
    clip-path: var(--lane-clip);
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
  .over {
    position: absolute;
    top: 0;
    bottom: 0;
    z-index: 2;
    clip-path: var(--lane-clip);
    pointer-events: none;
  }
  .range {
    position: absolute;
    top: 0;
    bottom: 0;
    z-index: 1;
    background: var(--line);
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
    width: 10px;
    height: 18px;
    border-radius: 3px;
    background: var(--accent);
    transform: translateX(-50%);
    pointer-events: auto;
    cursor: grab;
    touch-action: none;
  }
</style>
