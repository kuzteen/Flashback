<script lang="ts">
  import Icon from './Icon.svelte';
  import { formatDuration } from '$lib/clips';
  import { requestThumb } from '$lib/library.svelte';
  import {
    shareState,
    closeShare,
    selectPreset,
    startDrag,
    presetDisabled,
    SIZE_PRESETS
  } from '$lib/share.svelte';
  import { t } from '$lib/i18n.svelte';

  const clip = $derived(shareState.clip);

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
        <h2 class="title">{t('share.title')}</h2>
        <button class="close" aria-label={t('share.close')} onclick={closeShare}>
          <Icon name="close" size={16} sw={2} />
        </button>
      </header>

      <p class="label">{t('share.sizeLabel')}</p>
      <div class="sizes" role="group" aria-label={t('share.sizeLabel')}>
        <button class="size" class:on={shareState.preset === null} onclick={() => selectPreset(null)}>
          {t('share.original')}
        </button>
        {#each SIZE_PRESETS as mb (mb)}
          {@const off = presetDisabled(mb)}
          <!-- El botón deshabilitado no recibe el ratón: el hover lo recoge el contenedor. -->
          <span class="slot" class:off>
            <button class="size" class:on={shareState.preset === mb} disabled={off} onclick={() => selectPreset(mb)}>
              {mb} MB
            </button>
            {#if off}
              <span class="tip" role="tooltip">{t('share.alreadySmaller', { mb })}</span>
            {/if}
          </span>
        {/each}
      </div>

      <div class="pitch">
        <span class="pitch-title">{t('share.dragTitle')}</span>
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

      {#if shareState.error}
        <p class="error">{shareState.error}</p>
      {/if}
    </div>
  </div>
{/if}

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
    padding: 18px 20px 20px;
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
  .title {
    font-family: var(--font-mono);
    font-size: 12.5px;
    font-weight: 600;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--text-0);
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

  .label {
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
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    cursor: pointer;
    transition: color 0.14s ease, background 0.14s ease, border-color 0.14s ease;
  }
  .size:hover:not(:disabled):not(.on) {
    color: var(--text-0);
    border-color: var(--line-strong);
  }
  .size.on {
    color: var(--on-accent);
    background: var(--accent);
    border-color: transparent;
    font-weight: 560;
  }
  .size.on:hover {
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
  /* Tooltip propio: el title nativo no sale en un botón deshabilitado (no recibe el ratón). */
  .tip {
    position: absolute;
    bottom: calc(100% + 8px);
    left: 50%;
    transform: translateX(-50%);
    width: max-content;
    max-width: 240px;
    padding: 6px 10px;
    font-size: 12px;
    line-height: 1.3;
    color: var(--text-1);
    background: var(--bg-0);
    border: 1px solid var(--line-strong);
    border-radius: 7px;
    box-shadow: var(--shadow-float);
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    transition: opacity 0.12s ease, visibility 0.12s;
    z-index: 5;
  }
  .slot.off:hover .tip {
    opacity: 1;
    visibility: visible;
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
    text-align: center;
  }
  .pitch-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-0);
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
