<script module lang="ts">
  export type CoverChange = { bytes: Uint8Array | null; clear: boolean };
</script>

<script lang="ts">
  import { flip } from '$lib/flip';
  import { untrack, type Snippet } from 'svelte';
  import Icon from './Icon.svelte';
  import { t } from '$lib/i18n.svelte';

  // Esqueleto común de los diálogos con portada (playlist, editar clip): portada con su menú y
  // paso de recorte, campos a la derecha que pone quien lo usa, y Cancelar / Guardar. La portada
  // nueva o su borrado quedan pendientes hasta guardar, para que Cancelar lo deshaga todo.
  let {
    open,
    session,
    title,
    coverSrc,
    placeholder,
    fields,
    saveLabel,
    canSave,
    onsave,
    oncancel
  }: {
    open: boolean;
    // Cambia al abrir otro elemento: descarta la portada pendiente y el recorte del anterior.
    session: string;
    title: string;
    coverSrc: string | null;
    placeholder: Snippet;
    fields: Snippet;
    saveLabel: string;
    canSave: boolean;
    onsave: (cover: CoverChange) => Promise<void>;
    oncancel: () => void;
  } = $props();

  let fileEl = $state<HTMLInputElement | null>(null);
  let coverMenu = $state(false);
  let coverMenuEl = $state<HTMLElement | null>(null);
  let working = $state(false);
  // Bytes para el backend y URL de objeto para la vista previa, hasta confirmar.
  let pendingCover = $state<Uint8Array | null>(null);
  let pendingUrl = $state<string | null>(null);
  // Quitar la portada también espera a guardar: si se aplicara al momento, Cancelar no podría
  // devolverla porque el PNG ya no estaría en disco.
  let pendingClear = $state(false);

  const shownCover = $derived(pendingClear ? null : (pendingUrl ?? coverSrc));
  const hasCover = $derived(!!shownCover);

  $effect(() => {
    if (!coverMenu) return;
    const onDown = (e: MouseEvent) => {
      if (coverMenuEl && !coverMenuEl.contains(e.target as Node)) coverMenu = false;
    };
    window.addEventListener('mousedown', onDown, true);
    return () => window.removeEventListener('mousedown', onDown, true);
  });

  function dropPending() {
    if (pendingUrl) URL.revokeObjectURL(pendingUrl);
    pendingUrl = null;
    pendingCover = null;
    pendingClear = false;
  }

  $effect(() => {
    void session;
    void open;
    untrack(() => {
      closeCrop();
      dropPending();
      coverMenu = false;
    });
  });

  // La portada se guarda a 256 px: se ve pequeña en las tarjetas y a 180 aquí, así que cualquier
  // cosa mayor solo ocuparía disco y obligaría al WebView a decodificar de más en cada pintado.
  const COVER_PX = 256;
  const CROP_PX = 264;
  const MAX_ZOOM = 4;

  // Paso de recorte: la foto se mueve y amplía sobre un marco cuadrado y solo al aplicar se
  // rasteriza a COVER_PX, así que encuadrar no degrada la imagen.
  let cropUrl = $state<string | null>(null);
  let cropEl = $state<HTMLImageElement | null>(null);
  let nat = $state<{ w: number; h: number } | null>(null);
  let zoom = $state(1);
  let tx = $state(0);
  let ty = $state(0);
  let drag: { x: number; y: number; tx: number; ty: number } | null = null;

  // Encaje "cover": el lado corto llena el marco, de modo que zoom 1 nunca deja hueco.
  const fit = $derived(nat ? Math.max(CROP_PX / nat.w, CROP_PX / nat.h) : 1);
  const scale = $derived(fit * zoom);

  function clampOffsets() {
    if (!nat) return;
    const maxX = Math.max(0, (nat.w * scale - CROP_PX) / 2);
    const maxY = Math.max(0, (nat.h * scale - CROP_PX) / 2);
    tx = Math.min(maxX, Math.max(-maxX, tx));
    ty = Math.min(maxY, Math.max(-maxY, ty));
  }

  function setZoom(z: number) {
    const next = Math.min(MAX_ZOOM, Math.max(1, z));
    // El desplazamiento se escala con el zoom: así lo que está en el centro del marco sigue
    // estando ahí al ampliar, en vez de huir hacia una esquina.
    const k = next / zoom;
    zoom = next;
    tx *= k;
    ty *= k;
    clampOffsets();
  }

  function closeCrop() {
    if (cropUrl) URL.revokeObjectURL(cropUrl);
    cropUrl = null;
    nat = null;
    drag = null;
  }

  function onCropLoad(e: Event) {
    const img = e.currentTarget as HTMLImageElement;
    nat = { w: img.naturalWidth, h: img.naturalHeight };
    zoom = 1;
    tx = 0;
    ty = 0;
  }

  function onPointerDown(e: PointerEvent) {
    if (!nat) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { x: e.clientX, y: e.clientY, tx, ty };
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    tx = drag.tx + (e.clientX - drag.x);
    ty = drag.ty + (e.clientY - drag.y);
    clampOffsets();
  }

  const NUDGE = 8;
  function onFrameKey(e: KeyboardEvent) {
    const pan: Record<string, [number, number]> = {
      ArrowLeft: [-NUDGE, 0],
      ArrowRight: [NUDGE, 0],
      ArrowUp: [0, -NUDGE],
      ArrowDown: [0, NUDGE]
    };
    if (pan[e.key]) {
      e.preventDefault();
      tx += pan[e.key][0];
      ty += pan[e.key][1];
      clampOffsets();
    } else if (e.key === '+' || e.key === '-') {
      e.preventDefault();
      setZoom(zoom * (e.key === '+' ? 1.12 : 1 / 1.12));
    }
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    setZoom(zoom * (e.deltaY < 0 ? 1.12 : 1 / 1.12));
  }

  function onFile(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    // El input se vacía siempre: si no, elegir el mismo archivo dos veces seguidas no dispara
    // un segundo change y parecería que la app lo ignora.
    input.value = '';
    if (!file || working || !open) return;
    closeCrop();
    cropUrl = URL.createObjectURL(file);
  }

  async function applyCrop() {
    const img = cropEl;
    if (!img || !nat || working) return;
    working = true;
    try {
      // El marco ve un cuadrado de CROP_PX/escala píxeles de la imagen, centrado donde la haya
      // dejado el arrastre; ese recorte es el que se rasteriza.
      const side = CROP_PX / scale;
      const sx = nat.w / 2 - tx / scale - side / 2;
      const sy = nat.h / 2 - ty / scale - side / 2;
      const canvas = document.createElement('canvas');
      canvas.width = COVER_PX;
      canvas.height = COVER_PX;
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      ctx.drawImage(img, sx, sy, side, side, 0, 0, COVER_PX, COVER_PX);
      const blob = await new Promise<Blob | null>((res) => canvas.toBlob(res, 'image/png'));
      if (!blob) return;
      const bytes = new Uint8Array(await blob.arrayBuffer());
      dropPending();
      pendingCover = bytes;
      pendingUrl = URL.createObjectURL(blob);
      closeCrop();
    } catch (err) {
      console.error('cover', err);
    } finally {
      working = false;
    }
  }

  function clearCover() {
    const had = !!coverSrc;
    dropPending();
    // Sobre algo ya guardado solo queda anotado; el PNG se borra al confirmar.
    pendingClear = had;
  }

  export async function save() {
    if (!canSave || working) return;
    working = true;
    try {
      await onsave({ bytes: pendingCover, clear: pendingClear });
    } finally {
      working = false;
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    if (!open || e.key !== 'Escape') return;
    // El editor también escucha Escape en window y cerraría los dos con una sola tecla si el
    // evento siguiera propagándose.
    e.preventDefault();
    e.stopImmediatePropagation();
    // Con el menú de la portada abierto, Escape cierra solo el menú; en el recorte descarta solo
    // ese paso, porque cerrar el diálogo entero perdería lo ya escrito.
    if (coverMenu) coverMenu = false;
    else if (cropUrl) closeCrop();
    else oncancel();
  }
</script>

<svelte:window onkeydown={onKeyDown} />

{#if open}
  <div
    class="backdrop"
    role="presentation"
    onclick={(e) => {
      if (e.target !== e.currentTarget) return;
      if (cropUrl) closeCrop();
      else oncancel();
    }}
  >
    <div class="card" role="dialog" aria-modal="true" aria-label={title}>
      <h2 class="modal-title title">{cropUrl ? t('pl.cropTitle') : title}</h2>

      {#if cropUrl}
        <div class="crop">
          <button
            type="button"
            class="frame"
            aria-label={t('pl.cropTitle')}
            onkeydown={onFrameKey}
            onpointerdown={onPointerDown}
            onpointermove={onPointerMove}
            onpointerup={() => (drag = null)}
            onpointercancel={() => (drag = null)}
            onwheel={onWheel}
          >
            <img
              bind:this={cropEl}
              src={cropUrl}
              alt=""
              draggable="false"
              onload={onCropLoad}
              style:visibility={nat ? 'visible' : 'hidden'}
              style:width={nat ? `${nat.w}px` : 'auto'}
              style:height={nat ? `${nat.h}px` : 'auto'}
              style:transform="translate(-50%, -50%) translate({tx}px, {ty}px) scale({scale})"
            />
          </button>
          <input
            class="zoom"
            type="range"
            min="1"
            max={MAX_ZOOM}
            step="0.01"
            value={zoom}
            aria-label={t('pl.cropZoom')}
            oninput={(e) => setZoom(Number(e.currentTarget.value))}
          />
          <span class="crop-hint mono">{t('pl.cropHint')}</span>
        </div>

        <div class="actions">
          <button class="btn" onclick={closeCrop}>{t('confirm.cancel')}</button>
          <button class="btn primary" disabled={!nat || working} onclick={applyCrop}>
            {t('pl.cropApply')}
          </button>
        </div>
      {:else}
        <div class="body">
          <div class="cover-wrap">
            <button class="cover" class:busy={working} aria-label={t('pl.coverPick')} onclick={() => fileEl?.click()}>
              {#if shownCover}
                <img src={shownCover} alt="" draggable="false" />
              {:else}
                {@render placeholder()}
              {/if}
              <span class="cover-hint"><Icon name="camera-swap" size={30} /></span>
            </button>
            {#if hasCover}
              <div class="cover-more" class:open={coverMenu} bind:this={coverMenuEl}>
                <button
                  class="dots"
                  aria-label={t('card.more')}
                  aria-haspopup="menu"
                  aria-expanded={coverMenu}
                  onclick={() => (coverMenu = !coverMenu)}
                >
                  <Icon name="more" size={18} />
                </button>
                {#if coverMenu}
                  <div class="cover-menu" role="menu" use:flip>
                    <button
                      role="menuitem"
                      onclick={() => {
                        coverMenu = false;
                        fileEl?.click();
                      }}
                    >
                      <Icon name="camera-swap" size={16} />
                      {t('pl.coverChange')}
                    </button>
                    <button
                      role="menuitem"
                      class="danger"
                      onclick={() => {
                        coverMenu = false;
                        clearCover();
                      }}
                    >
                      <Icon name="trash" size={16} sw={2} />
                      {t('pl.coverClear')}
                    </button>
                  </div>
                {/if}
              </div>
            {/if}
            <input class="file" type="file" accept="image/*" bind:this={fileEl} onchange={onFile} />
          </div>

          <div class="fields">
            {@render fields()}
          </div>
        </div>

        <div class="actions">
          <button class="btn" onclick={oncancel}>{t('confirm.cancel')}</button>
          <button class="btn primary" disabled={!canSave || working} onclick={save}>{saveLabel}</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 220;
    display: grid;
    place-items: center;
    background: var(--scrim);
  }
  .card {
    width: 520px;
    max-width: calc(100vw - 40px);
    padding: 20px 22px 18px;
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-dialog);
    animation: dlg-in 0.16s ease-out;
  }
  @keyframes dlg-in {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .card {
      animation: none;
    }
  }

  .title {
    text-align: center;
  }

  /* Alturas fijas y no derivadas: la portada mide exactamente lo que los campos juntos, así que
     las dos columnas se leen como un solo bloque. */
  .body {
    --field-h: 40px;
    --field-gap: 12px;
    --block-h: 180px;
    display: flex;
    gap: 16px;
    margin-top: 18px;
  }
  .cover-wrap {
    position: relative;
    flex: none;
    width: var(--block-h);
    height: var(--block-h);
  }
  .cover {
    position: relative;
    width: 100%;
    height: 100%;
    display: grid;
    place-items: center;
    overflow: hidden;
    color: var(--text-3);
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    transition: border-color 0.15s ease, color 0.15s ease;
  }
  .cover:hover {
    color: var(--text-1);
    border-color: var(--line-strong);
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .cover-hint {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--text-0);
    background: rgba(0, 0, 0, 0.55);
    opacity: 0;
    transition: opacity 0.15s ease;
  }
  .cover:hover .cover-hint,
  .cover.busy .cover-hint {
    opacity: 1;
  }
  /* Hermano de la portada y no hijo: un botón dentro de otro no es HTML válido. Aparece con el
     mismo hover que el velo de la cámara, así que no ensucia la portada en reposo, y se queda
     mientras su menú esté abierto. */
  .cover-more {
    position: absolute;
    top: 7px;
    right: 7px;
  }
  .dots {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    color: var(--text-0);
    background: rgba(0, 0, 0, 0.6);
    border-radius: 50%;
    opacity: 0;
    transition: opacity 0.15s ease, background 0.14s ease;
  }
  .cover-wrap:hover .dots,
  .cover-more.open .dots,
  .dots:focus-visible {
    opacity: 1;
  }
  .dots:hover,
  .cover-more.open .dots {
    background: rgba(0, 0, 0, 0.82);
  }
  .cover-menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    width: 190px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    z-index: 10;
  }
  .cover-menu button {
    display: flex;
    align-items: center;
    gap: 11px;
    width: 100%;
    padding: 9px 10px;
    font-size: 13px;
    color: var(--text-1);
    text-align: left;
    border-radius: 6px;
    transition: background 0.12s ease, color 0.12s ease;
  }
  .cover-menu button :global(svg) {
    flex-shrink: 0;
    width: 17px;
  }
  .cover-menu button:hover {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .cover-menu .danger {
    color: var(--rec);
  }
  .cover-menu .danger:hover {
    background: color-mix(in srgb, var(--rec) 12%, transparent);
    color: var(--rec);
  }
  .file {
    display: none;
  }

  .crop {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    margin-top: 18px;
  }
  /* El marco es el recorte: lo que se ve aquí es exactamente lo que se guarda. */
  .frame {
    position: relative;
    display: block;
    padding: 0;
    width: 264px;
    height: 264px;
    /* content-box a propósito: el recorte que se guarda asume que el área visible mide
       CROP_PX, y con border-box el borde se la comería. */
    box-sizing: content-box;
    overflow: hidden;
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    cursor: grab;
    touch-action: none;
  }
  .frame:active {
    cursor: grabbing;
  }
  .frame:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .frame img {
    position: absolute;
    top: 50%;
    left: 50%;
    max-width: none;
    transform-origin: center;
    user-select: none;
    -webkit-user-drag: none;
  }
  .zoom {
    width: 266px;
    height: 4px;
    appearance: none;
    background: var(--bg-3);
    border-radius: 2px;
    outline: none;
  }
  .zoom::-webkit-slider-thumb {
    appearance: none;
    width: 14px;
    height: 14px;
    background: var(--accent);
    border-radius: 50%;
    cursor: pointer;
  }
  .crop-hint {
    font-size: 10.5px;
    letter-spacing: 0.04em;
    color: var(--text-3);
  }

  /* Los campos los pone quien usa el diálogo; su aspecto es común a todos. */
  .fields {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--field-gap);
  }
  .fields :global(.field) {
    position: relative;
    display: block;
  }
  .fields :global(.field input),
  .fields :global(.field textarea) {
    display: block;
    width: 100%;
    padding: 0 12px;
    font-family: inherit;
    font-size: 13.5px;
    color: var(--text-0);
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    outline: none;
    resize: none;
    transition: border-color 0.14s ease;
  }
  .fields :global(.field input) {
    height: var(--field-h);
    font-weight: 600;
  }
  .fields :global(.field textarea) {
    height: calc(var(--block-h) - var(--field-h) - var(--field-gap));
    padding-top: 10px;
    padding-bottom: 24px;
    line-height: 1.45;
  }
  .fields :global(.field input:focus),
  .fields :global(.field textarea:focus) {
    border-color: var(--text-3);
  }
  .fields :global(.field input::placeholder),
  .fields :global(.field textarea::placeholder) {
    color: var(--text-3);
    font-weight: 400;
  }
  /* La etiqueta va encajada en el borde superior y solo al enfocar: en reposo el placeholder
     ya dice qué es cada campo. El fondo es mitad y mitad (diálogo arriba, campo abajo) para
     que corte el borde sin dejar una caja visible por fuera. */
  .fields :global(.notch) {
    position: absolute;
    top: 0;
    left: 9px;
    padding: 0 4px;
    transform: translateY(-50%);
    font-size: 11px;
    font-weight: 600;
    line-height: 1.2;
    color: var(--text-1);
    background: linear-gradient(to bottom, var(--bg-1) 50%, var(--bg-0) 50%);
    opacity: 0;
    transition: opacity 0.14s ease;
    pointer-events: none;
  }
  .fields :global(.field:focus-within .notch) {
    opacity: 1;
  }

  .actions {
    display: flex;
    justify-content: center;
    gap: 8px;
    margin-top: 24px;
  }
  /* Alto y ancho fijos, no derivados del texto: el relleno de acento hace que el botón
     primario se lea más grande, y con padding la diferencia real dependería de la etiqueta. */
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 128px;
    height: 36px;
    padding: 0 14px;
    font-size: 13px;
    color: var(--text-1);
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    transition: color 0.14s ease, background 0.14s ease, border-color 0.14s ease;
  }
  .btn:hover {
    color: var(--text-0);
    border-color: var(--line-strong);
  }
  .btn.primary {
    color: var(--on-accent);
    background: var(--accent);
    border-color: var(--accent);
  }
  .btn.primary:hover {
    background: var(--accent-soft);
    border-color: var(--accent-soft);
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
