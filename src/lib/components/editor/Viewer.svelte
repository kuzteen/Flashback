<script lang="ts">
  import { untrack } from 'svelte';
  import { beginGesture, commit, editorState, endGesture, preview, setDuration } from '$lib/editor-state.svelte';
  import { outToSeg, setFraming } from '$lib/edit-model';
  import { place, ZOOM_MAX, zoomOf } from '$lib/frame-math';
  import { isNeutral, sharpenKernel, svgColorValues } from '$lib/look';
  import { t } from '$lib/i18n.svelte';
  import { formatTimecode } from '$lib/timeline-math';
  import { playback } from './playback.svelte';
  import { ui } from './ui.svelte';
  import Transport from './Transport.svelte';
  import Icon from '../Icon.svelte';

  let video = $state<HTMLVideoElement | null>(null);
  let sys = $state<HTMLAudioElement | null>(null);
  let mic = $state<HTMLAudioElement | null>(null);

  // attach aplica también el volumen: al leer la mezcla, el efecto se repite con cada cambio de
  // fader o silencio y el audio sigue a la edición sin más cableado.
  $effect(() => {
    playback.attach(video, sys, mic);
  });

  type VfcVideo = HTMLVideoElement & {
    requestVideoFrameCallback?: (cb: (now: number, meta: { mediaTime: number }) => void) => number;
    cancelVideoFrameCallback?: (handle: number) => void;
  };

  // rVFC dispara en cada fotograma pintado, también tras buscar o avanzar en pausa, así que
  // shownMediaTime es siempre el del fotograma visible.
  $effect(() => {
    const v = video as VfcVideo | null;
    if (!v || typeof v.requestVideoFrameCallback !== 'function') return;
    playback.vfc = true;
    let alive = true;
    let handle = 0;
    const cb = (_now: number, meta: { mediaTime: number }) => {
      playback.shownMediaTime = meta.mediaTime;
      if (bgCanvas) drawBg();
      if (alive) handle = v.requestVideoFrameCallback!(cb);
    };
    handle = v.requestVideoFrameCallback(cb);
    return () => {
      alive = false;
      v.cancelVideoFrameCallback?.(handle);
    };
  });

  // Arranca solo en cuanto el clip está listo (metadatos, pistas y montaje), una vez por clip.
  let autoPlayed: string | null = null;
  $effect(() => {
    const src = editorState.videoSrc;
    if (!src || editorState.loading || !video || playback.kept <= 0 || autoPlayed === src) return;
    autoPlayed = src;
    untrack(() => void playback.play());
  });

  function onLoaded() {
    srcW = video?.videoWidth ?? 0;
    srcH = video?.videoHeight ?? 0;
    setDuration((video?.duration || 0) * 1000);
    playback.onReady();
  }

  const frac = $derived(playback.kept > 0 ? Math.min(1, playback.outPos / playback.kept) : 0);
  let progEl = $state<HTMLDivElement | null>(null);
  let progDrag = $state(false);
  // Posición bajo el puntero (0..1) para la burbuja de tiempo: se ve adónde se salta antes de
  // hacer clic.
  let hoverFrac = $state<number | null>(null);

  function fracAt(clientX: number): number {
    if (!progEl) return 0;
    const r = progEl.getBoundingClientRect();
    return Math.max(0, Math.min(1, (clientX - r.left) / Math.max(1, r.width)));
  }

  function progAt(clientX: number) {
    playback.seekOutput(fracAt(clientX) * playback.kept);
  }

  function onProgDown(e: PointerEvent) {
    if (e.button !== 0 || !progEl) return;
    e.preventDefault();
    progEl.setPointerCapture(e.pointerId);
    playback.pauseForSeek();
    progDrag = true;
    progAt(e.clientX);
  }

  function onProgMove(e: PointerEvent) {
    hoverFrac = fracAt(e.clientX);
    if (progDrag) progAt(e.clientX);
  }

  function onProgUp() {
    if (!progDrag) return;
    progDrag = false;
    playback.resumeAfterGesture();
  }

  // Lienzo virtual del formato vertical, en las mismas unidades que el export (1080x1920): el
  // visor escala esas cuentas al tamaño en pantalla con porcentajes.
  const OW = 1080;
  const OH = 1920;
  const SNAP_PX = 6;

  const format = $derived(editorState.edit.format);

  // Ajustes de imagen en la vista previa: un filtro SVG con la misma matriz y el mismo núcleo que
  // el export. Solo se engancha si hay algo que ajustar, para no sacar el vídeo del camino rápido
  // de composición del navegador sin motivo.
  const look = $derived(editorState.edit.look);
  const graded = $derived(!isNeutral(look));
  const kernel = $derived(sharpenKernel(look));
  const lookFilter = $derived(graded ? 'url(#fb-look)' : null);
  // Comparando, el mismo filtro se aplica solo a la derecha de la línea (subregión del filtro), así
  // no hace falta un segundo vídeo ni copiar fotogramas: el coste es el del filtro normal.
  const comparing = $derived(ui.compare && graded && !ui.fs);
  const videoFilter = $derived(comparing ? 'url(#fb-look-split)' : lookFilter);
  // En pantalla completa se ve el vídeo tal cual: el lienzo vertical es una herramienta de encuadre.
  const vert = $derived(format.kind === 'vertical' && !ui.fs);
  const zoom = $derived(zoomOf(format));

  let srcW = $state(0);
  let srcH = $state(0);

  // El encuadre que se ve y se arrastra es el del bloque bajo el cabezal.
  const segIdx = $derived(outToSeg(editorState.edit.segments, playback.outPos).index);
  const seg = $derived(editorState.edit.segments[segIdx]);
  const fg = $derived(
    srcW > 0 ? place(srcW, srcH, OW, OH, zoom, seg?.cropX ?? 0.5, seg?.cropY ?? 0.5) : { x: 0, y: 0, w: OW, h: OH },
  );
  // Si el fotograma cubre el lienzo, el fondo desenfocado no se ve y no hace falta pintarlo.
  const covers = $derived(fg.x <= 0 && fg.y <= 0 && fg.x + fg.w >= OW && fg.y + fg.h >= OH);

  let frameEl = $state<HTMLDivElement | null>(null);
  let stageEl = $state<HTMLDivElement | null>(null);

  // Zona donde se dibuja la línea (el vídeo, o el lienzo vertical) y caja del vídeo, las dos en
  // coordenadas del escenario. Se miden por fotograma mientras se compara (el vídeo cambia de
  // tamaño con los paneles y, en vertical, se mueve al encuadrar), pero la línea y el corte del
  // filtro salen los dos de `ui.split` en el mismo cálculo: si el corte se recalculara en la
  // medición, iría un fotograma por detrás de la línea al arrastrar.
  let region = $state({ x: 0, y: 0, w: 0, h: 0 });
  let vbox = $state({ x: 0, w: 0 });
  const videoSplit = $derived(
    vbox.w > 0 ? Math.min(1, Math.max(0, (region.x + ui.split * region.w - vbox.x) / vbox.w)) : ui.split
  );
  // El tirador no sale nunca de la zona: en los extremos se queda entero aunque la línea llegue
  // al borde.
  // En los extremos la pastilla pierde una flecha y encoge: ahí se ancla por su lado exterior (sobre
  // la línea, que está en el borde) para encoger hacia dentro sin asomar fuera del vídeo.
  const KNOB_R = 15;
  const knobAnchor = $derived(ui.split <= 0 ? 0 : ui.split >= 1 ? 1 : 0.5);
  const knobShift = $derived(
    knobAnchor !== 0.5
      ? 0
      : Math.min(Math.max(ui.split * region.w, KNOB_R), Math.max(KNOB_R, region.w - KNOB_R)) - ui.split * region.w
  );

  $effect(() => {
    if (!comparing) return;
    let raf = 0;
    const measure = () => {
      const st = stageEl?.getBoundingClientRect();
      const vr = video?.getBoundingClientRect();
      const rr = vert ? frameEl?.getBoundingClientRect() : vr;
      if (st && vr && rr && vr.width > 0) {
        const next = { x: rr.left - st.left, y: rr.top - st.top, w: rr.width, h: rr.height };
        if (next.x !== region.x || next.y !== region.y || next.w !== region.w || next.h !== region.h) region = next;
        const vx = vr.left - st.left;
        if (vx !== vbox.x || vr.width !== vbox.w) vbox = { x: vx, w: vr.width };
      }
      raf = requestAnimationFrame(measure);
    };
    measure();
    return () => cancelAnimationFrame(raf);
  });

  let splitDrag = false;

  function splitAt(clientX: number) {
    const st = stageEl?.getBoundingClientRect();
    if (!st || region.w <= 0) return;
    ui.split = Math.min(1, Math.max(0, (clientX - st.left - region.x) / region.w));
  }

  function onSplitDown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    splitDrag = true;
    splitAt(e.clientX);
  }

  function onSplitMove(e: PointerEvent) {
    if (splitDrag) splitAt(e.clientX);
  }

  function onSplitUp() {
    splitDrag = false;
  }

  function onSplitKey(e: KeyboardEvent) {
    const step = e.shiftKey ? 0.1 : 0.02;
    if (e.key === 'ArrowLeft') ui.split = Math.max(0, ui.split - step);
    else if (e.key === 'ArrowRight') ui.split = Math.min(1, ui.split + step);
    else return;
    e.preventDefault();
  }
  let guideX = $state(false);
  let guideY = $state(false);
  let drag: { sx: number; sy: number; px: number; py: number; index: number; moved: boolean } | null = null;

  // Posición (0..1) que produjo un borde ya colocado; se parte de ella y no del valor guardado
  // para que, si el fotograma estaba contra un borde, arrastrar hacia dentro responda al instante.
  function axisPos(start: number, size: number, canvas: number): number {
    return size > canvas ? (canvas / 2 - start) / size : (start + size / 2) / canvas;
  }

  // Recorrido útil de la posición en un eje: fuera de él place() ya no movería nada.
  function axisRange(size: number, canvas: number): [number, number] {
    const half = size > canvas ? canvas / (2 * size) : size / (2 * canvas);
    return [half, 1 - half];
  }

  // Mueve un eje d unidades del lienzo. Mayor que el lienzo: arrastrar a la derecha enseña lo de
  // la izquierda. Menor: el fotograma sigue al puntero. Con snap, a menos de SNAP_PX del centro se
  // pega a él, como las guías de Photoshop; k pasa de unidades del lienzo a px en pantalla.
  function moveAxis(p0: number, d: number, size: number, canvas: number, k: number, snap: boolean) {
    const [lo, hi] = axisRange(size, canvas);
    if (hi - lo < 1e-6) return { p: 0.5, snapped: false };
    const raw = size > canvas ? p0 - d / size : p0 + d / canvas;
    let p = Math.max(lo, Math.min(hi, raw));
    const span = size > canvas ? size : canvas;
    const snapped = snap && Math.abs(p - 0.5) * span * k < SNAP_PX;
    if (snapped) p = 0.5;
    return { p, snapped };
  }

  function onFrameDown(e: PointerEvent) {
    if (e.button !== 0 || !frameEl) return;
    e.preventDefault();
    frameEl.setPointerCapture(e.pointerId);
    drag = {
      sx: e.clientX,
      sy: e.clientY,
      px: axisPos(fg.x, fg.w, OW),
      py: axisPos(fg.y, fg.h, OH),
      index: segIdx,
      moved: false,
    };
  }

  function onFrameMove(e: PointerEvent) {
    if (!drag || !frameEl) return;
    const dx = e.clientX - drag.sx;
    const dy = e.clientY - drag.sy;
    if (!drag.moved) {
      if (Math.hypot(dx, dy) < 3) return;
      drag.moved = true;
      beginGesture();
    }
    const k = frameEl.clientWidth / OW;
    // Alt suelta el imán, como en la línea de tiempo.
    const nx = moveAxis(drag.px, dx / k, fg.w, OW, k, !e.altKey);
    const ny = moveAxis(drag.py, dy / k, fg.h, OH, k, !e.altKey);
    guideX = nx.snapped;
    guideY = ny.snapped;
    preview({ ...editorState.edit, segments: setFraming(editorState.edit.segments, drag.index, nx.p, ny.p) });
  }

  // Un clic sin arrastrar sigue siendo reproducir / pausar, como sobre el vídeo en horizontal.
  function onFrameUp() {
    const d = drag;
    drag = null;
    guideX = guideY = false;
    if (!d) return;
    if (d.moved) endGesture();
    else playback.toggle();
  }

  function onFrameKey(e: KeyboardEvent) {
    const sx = e.key === 'ArrowLeft' ? -1 : e.key === 'ArrowRight' ? 1 : 0;
    const sy = e.key === 'ArrowUp' ? -1 : e.key === 'ArrowDown' ? 1 : 0;
    if (!sx && !sy) return;
    e.preventDefault();
    e.stopPropagation();
    const step = 20;
    // Las flechas mueven la ventana del recorte sobre el vídeo, no el vídeo: en el eje donde el
    // fotograma sobresale, eso es el sentido contrario al de arrastrar.
    const dir = (size: number, canvas: number) => (size > canvas ? -1 : 1);
    const nx = moveAxis(axisPos(fg.x, fg.w, OW), dir(fg.w, OW) * sx * step, fg.w, OW, 0, false);
    const ny = moveAxis(axisPos(fg.y, fg.h, OH), dir(fg.h, OH) * sy * step, fg.h, OH, 0, false);
    commit({ ...editorState.edit, segments: setFraming(editorState.edit.segments, segIdx, nx.p, ny.p) });
  }

  // Rueda sobre el marco = zoom personalizado. Toda una tanda de rueda es un solo paso de deshacer:
  // se cierra cuando la rueda lleva un rato quieta.
  // Cada muesca suma o resta un 10 % exacto (67 → 57), solo limitado por los extremos; los deltas
  // pequeños de un touchpad se acumulan hasta sumar una muesca.
  let wheelEnd: ReturnType<typeof setTimeout> | null = null;
  let wheelAcc = 0;
  const NOTCH = 100;

  function onFrameWheel(e: WheelEvent) {
    if (format.kind !== 'vertical' || format.fill !== 'custom' || drag) return;
    wheelAcc += e.deltaMode === 1 ? e.deltaY * 33 : e.deltaY;
    const notches = Math.trunc(wheelAcc / NOTCH);
    if (!notches) return;
    wheelAcc -= notches * NOTCH;
    const pct = Math.round((format.zoom ?? 0.5) * 100) - notches * 10;
    const z = Math.max(0, Math.min(ZOOM_MAX * 100, pct)) / 100;
    if (wheelEnd) clearTimeout(wheelEnd);
    else beginGesture();
    preview({ ...editorState.edit, format: { kind: 'vertical', fill: 'custom', zoom: z } });
    wheelEnd = setTimeout(() => {
      wheelEnd = null;
      wheelAcc = 0;
      endGesture();
    }, 350);
  }

  // Encajado: el fondo es un lienzo diminuto al que se copia el fotograma, escalado y desenfocado
  // por CSS. Sin segundo decodificador y todo en la GPU del navegador.
  let bgCanvas = $state<HTMLCanvasElement | null>(null);

  function drawBg() {
    const c = bgCanvas;
    const v = video;
    if (!c || !v || !v.videoWidth) return;
    const ctx = c.getContext('2d');
    if (!ctx) return;
    const sw = Math.min(v.videoWidth, (v.videoHeight * 9) / 16);
    ctx.drawImage(v, (v.videoWidth - sw) / 2, 0, sw, v.videoHeight, 0, 0, c.width, c.height);
  }

  $effect(() => {
    if (vert && !covers && bgCanvas) drawBg();
  });
</script>

<svg class="defs" aria-hidden="true">
  <filter id="fb-look" color-interpolation-filters="sRGB">
    <feColorMatrix type="matrix" values={svgColorValues(look)} />
    {#if kernel}
      <feConvolveMatrix order="3" kernelMatrix={kernel.join(' ')} preserveAlpha="true" edgeMode="duplicate" />
    {/if}
  </filter>
  <filter
    id="fb-look-split"
    color-interpolation-filters="sRGB"
    primitiveUnits="objectBoundingBox"
    x="0"
    y="0"
    width="1"
    height="1"
  >
    <feColorMatrix type="matrix" values={svgColorValues(look)} x={videoSplit} y="0" width={1 - videoSplit} height="1" result="graded" />
    {#if kernel}
      <feConvolveMatrix
        in="graded"
        order="3"
        kernelMatrix={kernel.join(' ')}
        preserveAlpha="true"
        edgeMode="duplicate"
        x={videoSplit}
        y="0"
        width={1 - videoSplit}
        height="1"
        result="sharp"
      />
    {/if}
    <feMerge>
      <feMergeNode in="SourceGraphic" />
      <feMergeNode in={kernel ? 'sharp' : 'graded'} />
    </feMerge>
  </filter>
</svg>

<div class="stage" bind:this={stageEl}>
  {#if editorState.videoSrc}
    <div class="fit" class:balance-l={ui.formatOpen && !ui.lookOpen} class:balance-r={ui.lookOpen && !ui.formatOpen}>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
      <div
        class="frame"
        class:vert
        bind:this={frameEl}
        role={vert ? 'group' : undefined}
        aria-label={vert ? t('ed.framing') : undefined}
        tabindex={vert ? 0 : undefined}
        onpointerdown={vert ? onFrameDown : undefined}
        onpointermove={vert ? onFrameMove : undefined}
        onpointerup={vert ? onFrameUp : undefined}
        onpointercancel={vert ? onFrameUp : undefined}
        onkeydown={vert ? onFrameKey : undefined}
        onwheel={vert ? onFrameWheel : undefined}
      >
        {#if vert && !covers}
          <div class="bg-clip" style:filter={lookFilter}><canvas bind:this={bgCanvas} class="bg" width="54" height="96"></canvas></div>
        {/if}
        <video
          bind:this={video}
          src={editorState.videoSrc}
          playsinline
          style:filter={videoFilter}
          class:cmp={comparing}
          class:fs={ui.fs}
          class:nocursor={ui.fs && !ui.fsCtrlShow}
          style:left={vert ? `${(fg.x / OW) * 100}%` : null}
          style:top={vert ? `${(fg.y / OH) * 100}%` : null}
          style:width={vert ? `${(fg.w / OW) * 100}%` : null}
          style:height={vert ? `${(fg.h / OH) * 100}%` : null}
          onloadedmetadata={onLoaded}
          onended={() => playback.pause()}
          onclick={vert ? undefined : () => playback.toggle()}
        ><track kind="captions" /></video>
        {#if vert && guideX}<span class="guide gx"></span>{/if}
        {#if vert && guideY}<span class="guide gy"></span>{/if}
      </div>
    </div>
    {#if comparing && region.w > 0}
      <div
        class="compare"
        class:flat={!vert}
        data-theme="dark"
        style:left="{region.x}px"
        style:top="{region.y}px"
        style:width="{region.w}px"
        style:height="{region.h}px"
      >
        <span class="tag before">{t('ed.compare.before')}</span>
        <span class="tag after">{t('ed.compare.after')}</span>
        <div
          class="split"
          style:left="{ui.split * 100}%"
          role="slider"
          tabindex="0"
          aria-label={t('ed.compare')}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(ui.split * 100)}
          onpointerdown={onSplitDown}
          onpointermove={onSplitMove}
          onpointerup={onSplitUp}
          onpointercancel={onSplitUp}
          ondblclick={() => (ui.split = 0.5)}
          onkeydown={onSplitKey}
        >
          <span class="split-knob" style:translate="calc({knobShift}px - {knobAnchor * 100}%) 0"><span class="arr" class:gone={ui.split <= 0}><Icon name="chevron-left" size={12} sw={2.6} /></span><span class="arr" class:gone={ui.split >= 1}><Icon name="chevron-right" size={12} sw={2.6} /></span></span>
        </div>
      </div>
    {/if}
  {/if}
  {#if editorState.system}
    <audio bind:this={sys} src={editorState.system} preload="auto"></audio>
  {/if}
  {#if editorState.mic}
    <audio bind:this={mic} src={editorState.mic} preload="auto"></audio>
  {/if}

  {#if editorState.loading}
    <div class="prep mono">{t('ed.preparingAudio')}</div>
  {:else if editorState.error}
    <div class="prep mono err">{t('ed.trackSplitError', { error: String(editorState.error) })}</div>
  {/if}
</div>

{#if ui.fs && ui.fsCtrlShow}
  <div class="fs-bar">
    <span class="fs-time mono">{formatTimecode(playback.outPos)}</span>
    <div
      class="fs-prog"
      class:active={progDrag || hoverFrac !== null}
      bind:this={progEl}
      role="presentation"
      onpointerdown={onProgDown}
      onpointermove={onProgMove}
      onpointerup={onProgUp}
      onpointercancel={onProgUp}
      onpointerleave={() => (hoverFrac = null)}
    >
      <div class="track">
        <div class="fill" style:width="{frac * 100}%"></div>
        <div class="knob" style:left="{frac * 100}%"></div>
      </div>
      {#if hoverFrac !== null}
        <span class="hover-time mono" style:left="{hoverFrac * 100}%">
          {formatTimecode(hoverFrac * playback.kept)}
        </span>
      {/if}
    </div>
    <span class="fs-time end mono">{formatTimecode(playback.kept)}</span>
  </div>
  <Transport floating />
{/if}

<style>
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    background: var(--base);
    overflow: hidden;
  }
  /* inset: 0 fija un alto definido para que el vídeo, con max-height: 100%, se reescale al
     cambiar el alto del panel inferior en vez de quedarse a tamaño fijo. */
  .fit {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 18px 40px;
  }
  /* Con un solo panel abierto se reserva su ancho también en el lado contrario, para que el vídeo
     siga centrado en la ventana; con los dos abiertos ya se compensan solos. */
  .fit.balance-l {
    padding-left: calc(40px + var(--format-w));
  }
  .fit.balance-r {
    padding-right: calc(40px + var(--format-w));
  }
  /* Capa sobre el vídeo: solo la línea recibe el puntero, el resto deja pasar el clic de
     reproducir y el arrastre del encuadre. */
  .compare {
    position: absolute;
    z-index: 3;
    pointer-events: none;
  }
  /* Chromium toma como caja del filtro el vídeo más lo que ocupa su sombra (~32 px por lado), y las
     fracciones del corte se medían sobre esa caja mayor: la línea y el corte solo coincidían en el
     centro. Comparando, la sombra la pinta esta capa, que ocupa justo la caja del vídeo. */
  video.cmp {
    box-shadow: none;
  }
  .compare.flat {
    border-radius: var(--r-md);
    box-shadow: 0 24px 60px -28px rgba(0, 0, 0, 0.9);
  }
  .tag {
    position: absolute;
    top: 10px;
    padding: 5px 8px;
    font-size: 11px;
    font-weight: 560;
    line-height: 1;
    text-box: trim-both cap alphabetic;
    color: var(--text-0);
    background: var(--glass);
    border: 1px solid var(--glass-edge);
    border-radius: var(--r-sm);
    backdrop-filter: blur(10px);
  }
  .before {
    left: 10px;
  }
  .after {
    right: 10px;
  }
  .split {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 24px;
    margin-left: -12px;
    pointer-events: auto;
    cursor: ew-resize;
    outline: none;
  }
  .split::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 10px;
    width: 4px;
    background: #fff;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.3), 0 0 12px rgba(0, 0, 0, 0.35);
  }
  .split-knob {
    position: absolute;
    top: 50%;
    left: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    height: 18px;
    margin-top: -9px;
    padding: 0 3px;
    color: #111;
    background: var(--accent-soft);
    border-radius: 4px;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.6);
    transition: transform 0.12s ease;
  }
  /* En un extremo desaparece la flecha que apunta hacia él, hueco incluido: por ahí ya no se puede
     tirar más, y la pastilla (de ancho automático) encoge con ella. */
  .arr {
    display: inline-flex;
    justify-content: center;
    width: 12px;
    overflow: hidden;
    transition: width 0.15s ease, opacity 0.15s ease;
  }
  .arr.gone {
    width: 0;
    opacity: 0;
  }
  .arr :global(svg) {
    flex-shrink: 0;
  }
  .split:hover .split-knob,
  .split:focus-visible .split-knob {
    transform: scale(1.08);
  }
  .defs {
    position: absolute;
    width: 0;
    height: 0;
  }
  video {
    max-width: 100%;
    max-height: 100%;
    display: block;
    cursor: pointer;
    border-radius: var(--r-md);
    box-shadow: 0 24px 60px -28px rgba(0, 0, 0, 0.9);
    object-fit: contain;
  }
  /* La ventana nativa ya cubre el monitor: el vídeo se fija al viewport entero y object-fit deja
     solo el letterbox real de su proporción. */
  video.fs {
    position: fixed;
    inset: 0;
    width: 100vw;
    height: 100vh;
    max-width: none;
    max-height: none;
    background: #000;
    border-radius: 0;
    box-shadow: none;
    z-index: 9999;
  }
  video.nocursor {
    cursor: none;
  }
  .prep {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    padding: 7px 12px;
    font-size: 12px;
    color: var(--text-1);
    background: var(--bg-2);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
  }
  .prep.err {
    color: var(--rec);
  }
  .fs-bar {
    position: fixed;
    left: 40px;
    right: 40px;
    bottom: 22px;
    display: flex;
    align-items: center;
    gap: 14px;
    z-index: 10000;
  }
  .fs-time {
    min-width: 64px;
    font-size: 12px;
    color: var(--text-0);
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.8);
  }
  .fs-time.end {
    text-align: right;
  }
  /* Fina en reposo y más gruesa al pasar o arrastrar; el punto solo aparece entonces. El área
     de clic es más alta que la pista para no tener que apuntar a 4 px. */
  .fs-prog {
    position: relative;
    flex: 1;
    height: 22px;
    display: flex;
    align-items: center;
    cursor: pointer;
    touch-action: none;
  }
  .track {
    position: relative;
    width: 100%;
    height: 4px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.22);
    transition: height 0.15s ease;
  }
  .fs-prog.active .track {
    height: 7px;
  }
  .fill {
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
  }
  .knob {
    position: absolute;
    top: 50%;
    width: 18px;
    height: 11px;
    border-radius: 3px;
    background: var(--accent-soft);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.6);
    transform: translate(-50%, -50%) scale(0);
    transition: transform 0.15s ease;
    pointer-events: none;
  }
  .fs-prog.active .knob {
    transform: translate(-50%, -50%) scale(1);
  }
  .hover-time {
    position: absolute;
    bottom: calc(100% + 6px);
    transform: translateX(-50%);
    padding: 4px 8px;
    font-size: 11.5px;
    color: var(--text-0);
    white-space: nowrap;
    background: rgba(18, 18, 20, 0.8);
    border: 1px solid var(--line);
    border-radius: 6px;
    pointer-events: none;
  }
  /* En horizontal el envoltorio no existe para el layout y el vídeo se coloca como siempre; en
     vertical pasa a ser el lienzo 9:16 y el vídeo se sitúa dentro con las cuentas del export. */
  .frame {
    display: contents;
  }
  .frame.vert {
    position: relative;
    display: block;
    height: 100%;
    aspect-ratio: 9 / 16;
    max-width: 100%;
    border-radius: var(--r-md);
    background: #000;
    box-shadow: 0 24px 60px -28px rgba(0, 0, 0, 0.9);
    cursor: grab;
    touch-action: none;
    outline: none;
  }
  .frame.vert:active {
    cursor: grabbing;
  }
  .frame.vert:focus-visible::after {
    box-shadow:
      inset 0 0 0 2px var(--accent),
      0 0 0 100vmax var(--outside);
  }
  /* Lo que el recorte deja fuera se sigue viendo, atenuado: así se sabe qué se pierde al encuadrar.
     El velo es la sombra del propio marco, que solo pinta por fuera de él. */
  .frame.vert::after {
    --outside: color-mix(in srgb, var(--base) 78%, transparent);
    content: '';
    position: absolute;
    inset: 0;
    z-index: 1;
    border-radius: inherit;
    box-shadow:
      inset 0 0 0 1px var(--line-strong),
      0 0 0 100vmax var(--outside);
    pointer-events: none;
  }
  .frame.vert video {
    position: absolute;
    max-width: none;
    max-height: none;
    object-fit: fill;
    border-radius: 0;
    box-shadow: none;
  }
  .bg-clip {
    position: absolute;
    inset: 0;
    border-radius: inherit;
    overflow: hidden;
  }
  .bg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    filter: blur(14px) brightness(0.65);
    transform: scale(1.15);
  }
  /* Guías de centro: aparecen solo mientras el vídeo está pegado al centro de ese eje. */
  .guide {
    position: absolute;
    z-index: 2;
    background: var(--gold);
    pointer-events: none;
  }
  .gx {
    top: 0;
    bottom: 0;
    left: 50%;
    width: 1px;
    margin-left: -0.5px;
  }
  .gy {
    left: 0;
    right: 0;
    top: 50%;
    height: 1px;
    margin-top: -0.5px;
  }
</style>
