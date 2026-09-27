<script lang="ts">
  import type { Snippet } from 'svelte';
  import { beginGesture, editorState, endGesture, preview } from '$lib/editor-state.svelte';
  import { moveTo, segLen, snap, snapTargets, sortByPos, trimToPos, type Segment } from '$lib/edit-model';
  import { SNAP_PX, blocksEnd, extentMs, outStartOf, posToOut } from '$lib/timeline-math';
  import { playback } from './playback.svelte';
  import { GRIP_COLORS, ui, type Track } from './ui.svelte';

  // Carril de bloques común a vídeo y audio: el audio va enlazado a los tramos del vídeo, así que
  // los tres carriles pintan los mismos bloques y cualquiera de ellos los recorta o los mueve.
  let {
    track,
    mpp,
    width,
    posAt,
    headPos,
    block,
    children,
  }: {
    track: Track;
    mpp: number;
    width: number;
    posAt: (clientX: number) => number;
    headPos: number;
    block: Snippet<[Segment, number]>;
    children?: Snippet;
  } = $props();

  // Mantener pulsado un bloque sin moverlo lo levanta para moverlo; si el puntero se mueve antes,
  // el gesto se queda en buscar.
  const LIFT_MS = 300;
  const LIFT_CANCEL_PX = 6;

  type Gesture =
    | { kind: 'scrub' }
    | { kind: 'pending'; index: number; downX: number; timer: ReturnType<typeof setTimeout> }
    | { kind: 'lift'; index: number; grab: number; extent: number }
    | { kind: 'trim'; index: number; edge: 'start' | 'end'; grab: number };

  let g: Gesture | null = null;
  let lane = $state<HTMLDivElement | null>(null);

  const segs = $derived(editorState.edit.segments);
  // Cada caja va ligada a su tramo y no a su puesto en la lista: al cortar, deshacer o reordenar
  // cambian los índices, y con la lista como clave las cajas pasaban a pintar otro tramo y se
  // deslizaban hasta él. El inicio del material que le pertenece es único (ningún material es de
  // dos bloques) y no cambia al mover ni al recortar; si alguna vez se repitiera, cae al índice.
  const keys = $derived.by(() => {
    const seen = new Set<number>();
    return segs.map((s, i) => {
      const k = seen.has(s.boundStartMs) ? -1 - i : s.boundStartMs;
      seen.add(k);
      return k;
    });
  });
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
      // El límite se fija al levantar: hasta el final del clip o del último bloque, para que uno
      // soltado más allá del final no salte atrás al agarrarlo, sin crecer mientras se arrastra.
      const extent = Math.max(extentMs(editorState.durationMs, ui.zoom), blocksEnd(editorState.edit.segments));
      g = { kind: 'lift', index: i, grab: posAt(downX) - s.posMs, extent };
      ui.lifted = i;
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
    ui.trimming = true;
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
      const r = moveTo(cur.segments, g.index, posAt(e.clientX) - g.grab, g.extent, snapMs(e), [headPos]);
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
      // Sin transición hasta que el reordenado ya esté pintado: dos fotogramas, porque en el
      // primero el navegador aún no ha aplicado los cambios.
      ui.settling = true;
      requestAnimationFrame(() => requestAnimationFrame(() => (ui.settling = false)));
      preview({ ...editorState.edit, segments: sortByPos(editorState.edit.segments) });
      const idx = editorState.edit.segments.findIndex((s) => s.posMs === pos);
      if (idx >= 0) editorState.active = idx;
      endGesture();
      playback.settle();
    } else if (cur.kind === 'trim') {
      endGesture();
    }
    ui.lifted = null;
    ui.trimming = false;
    ui.guideAt = null;
    playback.resumeAfterGesture();
  }

  function onContext(e: MouseEvent, i: number) {
    e.preventDefault();
    e.stopPropagation();
    editorState.active = i;
    ui.blockMenu = {
      x: Math.min(e.clientX, window.innerWidth - 190),
      y: Math.min(e.clientY, window.innerHeight - 150),
      index: i,
      track,
    };
  }
</script>

<div
  class="lane"
  class:trimming={ui.trimming}
  class:lifting={ui.lifted !== null}
  class:settling={ui.settling || ui.zooming}
  bind:this={lane}
  style:width="{width}px"
  style:--grip={GRIP_COLORS[ui.grips[track]].value}
  role="presentation"
  onpointerdown={onLaneDown}
  onpointermove={onMove}
  onpointerup={onUp}
  onpointercancel={onUp}
>
  {#each segs as s, i (keys[i])}
    <div
      class="block"
      class:active={i === editorState.active}
      class:disabled={s.disabled}
      class:lifted={ui.lifted === i}
      style:left="{px(s.posMs)}px"
      style:width="{px(segLen(s))}px"
      role="presentation"
      onpointerdown={(e) => onBlockDown(e, i)}
      oncontextmenu={(e) => onContext(e, i)}
    >
      <div class="content">{@render block(s, i)}</div>
      {#if i === editorState.active}
        <span class="grip start" role="presentation" onpointerdown={(e) => onGripDown(e, i, 'start')}></span>
        <span class="grip end" role="presentation" onpointerdown={(e) => onGripDown(e, i, 'end')}></span>
      {/if}
    </div>
  {/each}
  {@render children?.()}
</div>

<style>
  .lane {
    position: relative;
    flex: none;
    margin-left: var(--pad);
    clip-path: var(--lane-clip);
    cursor: pointer;
    touch-action: none;
  }
  /* 54 px de alto con 2 de aire arriba y abajo, sobre los 58 que deja la fila sin su línea. */
  .block {
    position: absolute;
    top: 2px;
    height: 54px;
    border-radius: 6px;
    background: linear-gradient(to bottom, var(--bg-3), var(--bg-2));
    transition: left 0.18s ease, box-shadow 0.14s ease, transform 0.14s ease;
  }
  /* El borde va por encima del contenido, que lo taparía si fuera del propio bloque. */
  .block::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    box-shadow: inset 0 0 0 1px var(--line-strong);
    transition: box-shadow 0.14s ease;
    pointer-events: none;
  }
  .content {
    position: absolute;
    inset: 0;
    border-radius: inherit;
    overflow: hidden;
  }
  .block.disabled .content {
    opacity: 0.3;
  }
  .lane.trimming .block,
  .lane.lifting .block {
    transition: box-shadow 0.14s ease, transform 0.14s ease;
  }
  .lane.settling .block,
  .lane.settling .block::after {
    transition: none;
  }
  /* El marco abraza también los tiradores, que van por fuera: sube por encima de los vecinos, y
     el contenido pierde las esquinas redondeadas para no dejar huecos contra el marco. */
  .block.active {
    z-index: 1;
  }
  .block.active::after {
    left: -12px;
    right: -12px;
    box-shadow: inset 0 0 0 3px var(--grip);
  }
  .block.active .content {
    border-radius: 0;
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
  /* Los tiradores forman los lados del marco por fuera del bloque, como en iOS: su borde interior
     es el corte y el bloque se ve y se agarra entero por corto que sea. La doble raya mide 6 px
     sobre 12 y 14 sobre los 54 del bloque, así que queda centrada en píxeles enteros. */
  .grip {
    position: absolute;
    z-index: 1;
    top: 0;
    bottom: 0;
    width: 12px;
    background: var(--grip);
    cursor: ew-resize;
  }
  .grip::before {
    content: '';
    position: absolute;
    inset: 0;
    width: 6px;
    height: 14px;
    margin: auto;
    background: linear-gradient(
      to right,
      var(--base) 0 2px,
      transparent 2px 4px,
      var(--base) 4px 6px
    );
  }
  .grip.start {
    left: -12px;
    border-radius: 6px 0 0 6px;
  }
  .grip.end {
    right: -12px;
    border-radius: 0 6px 6px 0;
  }
</style>
