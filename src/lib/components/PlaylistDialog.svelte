<script lang="ts">
  import { untrack } from 'svelte';
  import Icon from './Icon.svelte';
  import {
    playlistEdit,
    closePlaylistEdit,
    findPlaylist,
    createPlaylist,
    updatePlaylist,
    setPlaylistCover,
    clearPlaylistCover
  } from '$lib/playlists.svelte';
  import { t } from '$lib/i18n.svelte';

  const creating = $derived(playlistEdit.creating);
  const playlist = $derived(playlistEdit.id ? findPlaylist(playlistEdit.id) : undefined);
  const open = $derived(creating || !!playlist);

  let name = $state('');
  let description = $state('');
  let fileEl = $state<HTMLInputElement | null>(null);
  let working = $state(false);
  // Al crear todavía no hay id con el que nombrar el PNG, así que la portada se queda aquí
  // (bytes para el backend, URL de objeto para la vista previa) hasta confirmar.
  let pendingCover = $state<Uint8Array | null>(null);
  let pendingUrl = $state<string | null>(null);

  const coverSrc = $derived(creating ? pendingUrl : (playlist?.coverSrc ?? null));

  function dropPending() {
    if (pendingUrl) URL.revokeObjectURL(pendingUrl);
    pendingUrl = null;
    pendingCover = null;
  }

  // Los campos se siembran al abrir y al cerrar, no en cada render: leer la playlist sin
  // untrack volvería a sembrarlos con cada cambio de la lista (subir portada, por ejemplo) y
  // pisaría lo que el usuario está escribiendo.
  $effect(() => {
    const id = playlistEdit.id;
    void playlistEdit.creating;
    untrack(() => {
      closeCrop();
      dropPending();
      const p = id ? findPlaylist(id) : undefined;
      name = p?.name ?? '';
      description = p?.description ?? '';
    });
  });

  // El foco va por acción y no dentro del efecto: leer ahí la referencia del input lo haría
  // depender de ella, y al desmontarse el formulario (paso de recorte) el efecto se
  // reejecutaría cerrando el recorte y pisando lo escrito.
  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  const MAX_DESC = 180;
  // La portada se guarda a 256 px: se ve a 46 en la tarjeta y a 96 aquí, así que cualquier cosa
  // mayor solo ocuparía disco y obligaría al WebView a decodificar de más en cada pintado.
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
    if (!file || working || (!playlistEdit.id && !playlistEdit.creating)) return;
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
      if (playlistEdit.id) await setPlaylistCover(playlistEdit.id, bytes);
      else {
        dropPending();
        pendingCover = bytes;
        pendingUrl = URL.createObjectURL(blob);
      }
      closeCrop();
    } catch (err) {
      console.error('playlist cover', err);
    } finally {
      working = false;
    }
  }

  function clearCover() {
    if (playlistEdit.id) clearPlaylistCover(playlistEdit.id);
    else dropPending();
  }

  async function save() {
    if (!name.trim() || working) return;
    working = true;
    try {
      if (playlistEdit.id) await updatePlaylist(playlistEdit.id, name, description);
      else await createPlaylist(name, description, pendingCover);
    } finally {
      working = false;
    }
    closePlaylistEdit();
  }

  function onKeyDown(e: KeyboardEvent) {
    if (!playlistEdit.id && !playlistEdit.creating) return;
    if (e.key === 'Escape') {
      // Igual que ConfirmDialog: el editor también escucha Escape en window y cerraría los dos
      // con una sola tecla si el evento siguiera propagándose.
      e.preventDefault();
      e.stopImmediatePropagation();
      // Estando en el recorte, Escape descarta solo ese paso: cerrar el diálogo entero
      // perdería el nombre y la descripción ya escritos.
      if (cropUrl) closeCrop();
      else closePlaylistEdit();
    }
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
      else closePlaylistEdit();
    }}
  >
    <div class="card" role="dialog" aria-modal="true" aria-label={t(creating ? 'pl.new' : 'pl.edit')}>
      <h2 class="title">
        {cropUrl ? t('pl.cropTitle') : t(creating ? 'pl.new' : 'pl.edit')}
      </h2>

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
        <div class="cover-col">
          <button
            class="cover"
            class:busy={working}
            aria-label={t('pl.coverPick')}
            onclick={() => fileEl?.click()}
          >
            {#if coverSrc}
              <img src={coverSrc} alt="" draggable="false" />
            {:else}
              <Icon name="heart-fill" size={48} />
            {/if}
            <span class="cover-hint"><Icon name="camera-swap" size={30} /></span>
          </button>
          {#if coverSrc}
            <button class="cover-clear" onclick={clearCover}>
              {t('pl.coverClear')}
            </button>
          {/if}
          <input
            class="file"
            type="file"
            accept="image/*"
            bind:this={fileEl}
            onchange={onFile}
          />
        </div>

        <div class="fields">
          <label class="field">
            <span class="label">{t('pl.fieldName')}</span>
            <input
              use:focusSelect
              bind:value={name}
              maxlength="60"
              placeholder={t('pl.namePlaceholder')}
              onkeydown={(e) => e.key === 'Enter' && save()}
            />
          </label>
          <label class="field">
            <span class="label">{t('pl.fieldDesc')}</span>
            <textarea
              bind:value={description}
              maxlength={MAX_DESC}
              rows="3"
              placeholder={t('pl.descPlaceholder')}
            ></textarea>
            <span class="count mono">{description.length}/{MAX_DESC}</span>
          </label>
        </div>
      </div>

      <div class="actions">
        <button class="btn" onclick={closePlaylistEdit}>{t('confirm.cancel')}</button>
        <button class="btn primary" disabled={!name.trim()} onclick={save}>
          {t(creating ? 'pl.create' : 'pl.save')}
        </button>
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
    background: rgba(0, 0, 0, 0.62);
  }
  .card {
    width: 500px;
    max-width: calc(100vw - 40px);
    padding: 20px 22px 18px;
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: 0 24px 60px -18px rgba(0, 0, 0, 0.8);
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
    font-family: var(--font-mono);
    font-size: 12.5px;
    font-weight: 600;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--text-0);
  }

  .body {
    display: flex;
    gap: 18px;
    margin-top: 18px;
  }
  .cover-col {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
    flex: none;
  }
  /* Del alto del bloque de campos de al lado (nombre + descripción, con su hueco para el
     botón de quitar): la portada y la información se leen como un mismo bloque. */
  .cover {
    position: relative;
    width: 160px;
    height: 160px;
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
  /* Se ve como texto pero pulsa como botón: sin el relleno el clic solo entra justo encima
     de las letras. El hueco de la columna baja otro tanto para que no se separe más. */
  .cover-clear {
    display: inline-flex;
    align-items: center;
    padding: 6px 10px;
    border-radius: var(--r-sm);
    font-size: 11px;
    letter-spacing: 0.04em;
    color: var(--text-3);
    transition: color 0.14s ease;
  }
  .cover-clear:hover {
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

  .fields {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .field {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field input,
  .field textarea {
    width: 100%;
    padding: 8px 10px;
    font-family: inherit;
    font-size: 13.5px;
    line-height: 1.45;
    color: var(--text-0);
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    outline: none;
    resize: none;
    transition: border-color 0.14s ease;
  }
  .field textarea {
    padding-bottom: 22px;
  }
  .field input:focus,
  .field textarea:focus {
    border-color: var(--line-strong);
  }
  .field input::placeholder,
  .field textarea::placeholder {
    color: var(--text-3);
  }
  /* Dentro del recuadro: el textarea reserva sitio abajo y el contador lleva el fondo del
     campo para que el texto al desbordar no se lea por debajo. */
  .count {
    position: absolute;
    right: 9px;
    bottom: 7px;
    padding-left: 6px;
    font-size: 10.5px;
    color: var(--text-3);
    background: var(--bg-0);
    pointer-events: none;
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
