<script lang="ts">
  import Icon from './Icon.svelte';
  import { formatDuration } from '$lib/clips';
  import { requestThumb } from '$lib/library.svelte';
  import {
    shareState,
    closeShare,
    selectPreset,
    startDrag,
    copyShare,
    presetDisabled,
    SIZE_PRESETS
  } from '$lib/share.svelte';
  import { t } from '$lib/i18n.svelte';
  import MorphTip from './MorphTip.svelte';
  import { TipGroup } from '$lib/morph-tip.svelte';
  import { pill } from '$lib/pill';

  const clip = $derived(shareState.clip);
  const sizeTips = new TipGroup('top');

  // El botón pasa de "Copiar al portapapeles" a "Copiado" y vuelve solo. El ancho de las palabras
  // se mide para que el botón se estreche o ensanche con el texto en vez de saltar.
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  let copyW = $state(0);
  let copiedW = $state(0);

  async function onCopy() {
    if (!(await copyShare())) return;
    copied = true;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => (copied = false), 1800);
  }

  $effect(() => {
    if (!clip) {
      copied = false;
      clearTimeout(copyTimer);
    }
  });

  let poster = $state<string | null>(null);
  let cardEl = $state<HTMLElement | null>(null);

  // El modal se lleva el foco al abrirse: si se quedara en el botón de compartir de la tarjeta,
  // espacio volvería a activarlo por detrás del diálogo.
  $effect(() => {
    if (clip) cardEl?.focus();
  });

  // Solo el fotograma: aquí lo que importa es reconocer el clip y arrastrarlo, no reproducirlo.
  // Un <video> en hover además competiría con el gesto de arrastre por el mismo puntero.
  $effect(() => {
    const path = clip?.path;
    poster = null;
    if (!path) return;
    let alive = true;
    requestThumb(path).then((u) => {
      if (alive && u) poster = u;
    });
    return () => {
      alive = false;
    };
  });

  // Umbral antes de lanzar el arrastre nativo: sin él, un simple click sobre la zona iniciaría
  // una operación OLE modal que el usuario no ha pedido.
  const DRAG_THRESHOLD = 4;
  let origin: { x: number; y: number } | null = null;

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0 || shareState.preparing) return;
    origin = { x: e.clientX, y: e.clientY };
  }

  function onPointerMove(e: PointerEvent) {
    if (!origin) return;
    const moved = Math.hypot(e.clientX - origin.x, e.clientY - origin.y);
    if (moved < DRAG_THRESHOLD) return;
    origin = null;
    startDrag();
  }

  function onKeyDown(e: KeyboardEvent) {
    if (!shareState.clip || e.key !== 'Escape') return;
    // El editor también escucha Escape en window, y su listener corre después del de este diálogo
    // (se monta más tarde). Para entonces closeShare ya habría dejado shareState.clip en null y su
    // guard no vería el modal, cerrando los dos de una tecla. Se corta el evento aquí.
    e.preventDefault();
    e.stopImmediatePropagation();
    closeShare();
  }
</script>

<svelte:window onkeydown={onKeyDown} />

{#if clip}
  <div
    class="backdrop"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) closeShare();
    }}
  >
    <div
      class="card"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      aria-label={t('share.title')}
      bind:this={cardEl}
    >
      <header class="head">
        <h2 class="modal-title">{t('share.title')}</h2>
        <button class="close" aria-label={t('share.close')} onclick={closeShare}>
          <Icon name="close" size={16} sw={2} />
        </button>
      </header>

      <p class="label section">{t('share.sizeLabel')}</p>
      <div class="sizes" role="group" aria-label={t('share.sizeLabel')} use:pill={{ key: shareState.preset }}>
        <button class="size" class:on={shareState.preset === null} onclick={() => selectPreset(null)}>
          {t('share.original')}
        </button>
        {#each SIZE_PRESETS as mb (mb)}
          {@const off = presetDisabled(mb)}
          <!-- El botón deshabilitado no recibe el ratón: el hover lo recoge el contenedor. -->
          <span class="slot" class:off use:sizeTips.trigger={off ? t('share.alreadySmaller', { mb }) : ''}>
            <button
              class="size"
              class:on={shareState.preset === mb}
              disabled={off}
              onclick={() => selectPreset(mb)}
            >
              {mb} MB
            </button>
          </span>
        {/each}
      </div>

      <div class="pitch">
        <span class="label section">{t('share.dragTitle')}</span>
        <span class="pitch-sub">{t('share.dragHint')}</span>
      </div>

      <div
        class="drop"
        class:busy={shareState.preparing}
        role="button"
        tabindex="0"
        aria-label={t('share.dragHint')}
        draggable="false"
        onpointerdown={onPointerDown}
        onpointermove={onPointerMove}
        onpointerup={() => (origin = null)}
        onpointerleave={() => (origin = null)}
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') startDrag();
        }}
      >
        <div class="frame">
          {#if poster}
            <img class="media" src={poster} alt="" draggable="false" />
          {/if}

          <span class="dur mono">{formatDuration(shareState.durationSec)}</span>

          {#if shareState.preparing}
            <div class="veil busy-veil">
              <span class="veil-title">{t('share.preparing')}</span>
              <div class="track"><div class="fill" style:width={`${Math.max(2, Math.round(shareState.progress * 100))}%`}></div></div>
              <span class="veil-sub mono">{Math.round(shareState.progress * 100)}%</span>
              <button class="cancel" onclick={() => selectPreset(null)}>{t('share.cancel')}</button>
            </div>
          {:else}
            <div class="veil hint">
              <span class="veil-title">{t('share.dragTitle')}</span>
              <span class="veil-sub">{t('share.dragOver')}</span>
            </div>
          {/if}
        </div>
      </div>

      <button
        class="copy"
        class:copied
        aria-label={copied ? t('share.copied') : t('share.copy')}
        disabled={shareState.preparing}
        onclick={onCopy}
      >
        <span class="copy-icon" aria-hidden="true">
          <svg class="ic ic-copy" viewBox="0 0 16 16">
            <rect x="5.25" y="5.25" width="8.5" height="8.5" rx="1.75" />
            <path d="M10.75 5.25V3.75a1.5 1.5 0 0 0-1.5-1.5h-5.5a1.5 1.5 0 0 0-1.5 1.5v5.5a1.5 1.5 0 0 0 1.5 1.5h1.5" />
          </svg>
          <svg class="ic ic-check" viewBox="0 0 16 16">
            <path d="M3.5 8.25l3 3 6-6.5" />
          </svg>
        </span>
        <span class="words" style:width={copyW ? `${copied ? copiedW : copyW}px` : undefined} aria-hidden="true">
          <span class="word w-copy" bind:offsetWidth={copyW}>{t('share.copy')}</span>
          <span class="word w-copied" bind:offsetWidth={copiedW}>{t('share.copied')}</span>
        </span>
      </button>

      {#if shareState.error}
        <p class="error">{shareState.error}</p>
      {/if}
    </div>
  </div>
{/if}
<MorphTip group={sizeTips} />

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 210;
    display: grid;
    place-items: center;
    background: var(--scrim);
  }
  .card {
    width: 460px;
    max-width: calc(100vw - 40px);
    display: flex;
    flex-direction: column;
    padding: 18px 20px 16px;
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-dialog);
  }
  /* El contenedor solo recibe el foco para capturar el teclado, no es un control: sin anillo. */
  .card:focus {
    outline: none;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    margin-bottom: 18px;
  }
  .close {
    position: absolute;
    right: -4px;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: var(--r-sm);
    color: var(--text-2);
    cursor: pointer;
    transition: color 0.14s ease, background 0.14s ease;
  }
  .close:hover {
    color: var(--text-0);
    background: var(--bg-3);
  }

  /* Encabezado de cada bloque del diálogo (tamaño y arrastre): mismo estilo para los dos. */
  .section {
    margin-bottom: 8px;
    font-size: 12.5px;
    text-align: center;
    color: var(--text-2);
  }
  .sizes {
    display: grid;
    grid-template-columns: repeat(4, 84px);
    justify-content: center;
    gap: 6px;
    margin-bottom: 18px;
  }
  .size {
    height: 36px;
    font-size: 12.5px;
    color: var(--text-1);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    cursor: pointer;
    transition: color 0.18s ease 0.06s, background 0.14s ease, border-color 0.14s ease;
  }
  .size:hover:not(:disabled):not(.on) {
    color: var(--text-0);
    border-color: var(--line-strong);
  }
  .size.on {
    color: var(--on-accent);
    border-color: transparent;
    font-weight: 560;
  }
  .sizes > :global(.slide-pill) {
    background: var(--accent);
    border-radius: var(--r-sm);
  }
  .sizes:has(.size.on:hover) > :global(.slide-pill) {
    background: var(--accent-deep);
  }
  .size:disabled {
    opacity: 0.38;
    cursor: not-allowed;
  }
  .slot {
    position: relative;
    display: grid;
  }

  /* El borde discontinuo es la señal de "esto se arrastra"; al pasar por encima se vuelve sólido
     y se enciende el acento, igual que una zona de drop activa. */
  /* El borde discontinuo marca la zona de soltado y no se mueve; el fotograma vive un poco por
     dentro para que se lea como algo agarrable dentro de ella, no como el propio recuadro. */
  .pitch {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    margin-bottom: 14px;
    padding-top: 18px;
    border-top: 1px solid var(--line);
    text-align: center;
  }
  .pitch-sub {
    font-size: 12px;
    line-height: 1.35;
    color: var(--text-2);
  }

  /* El padding en porcentaje se resuelve contra el ancho también arriba y abajo, así que un único
     valor deja la misma separación en píxeles por los cuatro lados. El alto lo marca el fotograma,
     que es quien conserva el 16:9. */
  .drop {
    position: relative;
    display: grid;
    place-items: center;
    width: 100%;
    padding: 1.25%;
    background: var(--bg-0);
    border: 2px dashed var(--line-strong);
    border-radius: var(--r-md);
    cursor: grab;
  }
  .drop:active:not(.busy) {
    cursor: grabbing;
  }
  .drop.busy {
    cursor: progress;
  }
  .frame {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    overflow: hidden;
    background: var(--bg-0);
    border: 2px solid transparent;
    border-radius: calc(var(--r-md) - 2px);
    transition: border-color 0.14s ease;
  }
  /* El resalte del hover va pegado al fotograma, no al borde discontinuo de fuera. */
  .drop:hover:not(.busy) .frame {
    border-color: var(--line-strong);
  }

  .media {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    pointer-events: none;
  }
  .dur {
    position: absolute;
    top: 8px;
    right: 8px;
    z-index: 2;
    padding: 4px 9px;
    font-size: 11.5px;
    color: var(--text-0);
    background: rgba(0, 0, 0, 0.62);
    border-radius: 999px;
    pointer-events: none;
  }

  .veil {
    position: absolute;
    inset: 0;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 16px;
    text-align: center;
    background: rgba(0, 0, 0, 0.5);
    pointer-events: none;
  }
  /* El velo normal no intercepta el puntero para no estorbar al arrastre; el de preparación sí,
     porque contiene el botón de cancelar. */
  .busy-veil {
    pointer-events: auto;
  }
  .veil.hint {
    opacity: 0;
    transition: opacity 0.14s ease;
  }
  .drop:hover .veil.hint {
    opacity: 1;
  }
  @media (prefers-reduced-motion: reduce) {
    .frame {
      transition: none;
    }
  }
  .veil-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-0);
  }
  .veil-sub {
    font-size: 12px;
    color: var(--text-1);
  }
  .track {
    width: 60%;
    height: 6px;
    margin-bottom: 4px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.16);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.15s ease;
  }
  .cancel {
    margin-top: 10px;
    padding: 6px 14px;
    font-size: 12px;
    color: var(--text-1);
    background: rgba(0, 0, 0, 0.45);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    cursor: pointer;
    transition: color 0.14s ease, border-color 0.14s ease;
  }
  .cancel:hover {
    color: var(--text-0);
    border-color: var(--text-2);
  }

  .copy {
    --morph: cubic-bezier(0.23, 1, 0.32, 1);
    align-self: center;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    margin-top: 14px;
    padding: 0 14px 0 12px;
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text-1);
    background: var(--bg-0);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    transition: color 0.15s ease, background 0.15s ease, scale 0.15s ease;
  }
  .copy:hover:not(:disabled) {
    color: var(--text-0);
    background: var(--bg-2);
  }
  .copy:active:not(:disabled) {
    scale: 0.96;
  }
  .copy:disabled {
    opacity: 0.45;
  }
  .copy.copied {
    color: var(--text-0);
  }
  .copy-icon {
    display: grid;
    width: 16px;
    height: 16px;
  }
  .ic {
    grid-area: 1 / 1;
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linecap: round;
    stroke-linejoin: round;
    transition: opacity 0.2s var(--morph), scale 0.2s var(--morph), filter 0.2s var(--morph);
  }
  .ic-check {
    stroke-width: 1.7;
  }
  .ic-check,
  .copied .ic-copy {
    opacity: 0;
    scale: 0.25;
    filter: blur(4px);
  }
  .copied .ic-check {
    opacity: 1;
    scale: 1;
    filter: blur(0);
  }
  /* Las dos etiquetas ocupan la misma celda; el ancho visible viaja de una a otra. */
  .words {
    display: grid;
    overflow: hidden;
    transition: width 0.4s var(--morph);
  }
  .word {
    grid-area: 1 / 1;
    width: max-content;
    white-space: nowrap;
    transition: opacity 0.2s var(--morph) 0.06s, filter 0.2s var(--morph) 0.06s;
  }
  .w-copied,
  .copied .w-copy {
    opacity: 0;
    filter: blur(4px);
    transition-duration: 0.1s;
    transition-delay: 0s;
  }
  .copied .w-copied {
    opacity: 1;
    filter: blur(0);
    transition-duration: 0.2s;
    transition-delay: 0.06s;
  }
  .error {
    margin-top: 12px;
    font-size: 12px;
    line-height: 1.4;
    color: var(--rec-text);
  }

  @media (prefers-reduced-motion: reduce) {
    .veil.hint,
    .fill {
      transition: none;
    }
  }
</style>
