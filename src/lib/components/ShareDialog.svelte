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
  let hovering = $state(false);
  let videoReady = $state(false);

  // Mismo patrón que la tarjeta: póster ligero siempre, y el <video> solo montado en hover para
  // no dejar un decodificador vivo mientras el diálogo está abierto sin que nadie lo mire.
  $effect(() => {
    const path = clip?.path;
    poster = null;
    videoReady = false;
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
    if (e.key === 'Escape') closeShare();
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
    <div class="card" role="dialog" aria-modal="true" tabindex="-1" aria-label={t('share.title')}>
      <header class="head">
        <h2 class="title">{t('share.title')}</h2>
        <button class="close" aria-label={t('share.close')} onclick={closeShare}>
          <Icon name="close" size={16} sw={2} />
        </button>
      </header>

      <p class="label">{t('share.sizeLabel')}</p>
      <div class="sizes" role="group" aria-label={t('share.sizeLabel')}>
        <button
          class="size"
          class:on={shareState.preset === null}
          disabled={shareState.preparing}
          onclick={() => selectPreset(null)}
        >
          {t('share.original')}
        </button>
        {#each SIZE_PRESETS as mb (mb)}
          {@const off = presetDisabled(clip, mb)}
          <button
            class="size"
            class:on={shareState.preset === mb}
            disabled={off || shareState.preparing}
            title={off ? t('share.alreadySmaller') : ''}
            onclick={() => selectPreset(mb)}
          >
            {mb} MB
          </button>
        {/each}
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
        onmouseenter={() => (hovering = true)}
        onmouseleave={() => {
          hovering = false;
          videoReady = false;
        }}
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') startDrag();
        }}
      >
        {#if poster}
          <img class="media" class:hide={hovering && videoReady} src={poster} alt="" draggable="false" />
        {/if}
        {#if hovering && clip.previewSrc && !shareState.preparing}
          <video
            class="media vid"
            class:show={videoReady}
            src={clip.previewSrc}
            muted
            loop
            autoplay
            playsinline
            draggable="false"
            onloadeddata={() => (videoReady = true)}
          ></video>
        {/if}

        <span class="dur mono">{formatDuration(clip.durationSec)}</span>

        {#if shareState.preparing}
          <div class="veil">
            <div class="track"><div class="fill" style:width={`${Math.max(2, Math.round(shareState.progress * 100))}%`}></div></div>
            <span class="veil-sub mono">{Math.round(shareState.progress * 100)}%</span>
            <span class="veil-title">{t('share.preparing')}</span>
          </div>
        {:else}
          <div class="veil hint">
            <span class="veil-title">{t('share.dragTitle')}</span>
            <span class="veil-sub">{t('share.dragHint')}</span>
          </div>
        {/if}
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
    background: rgba(0, 0, 0, 0.62);
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
    box-shadow: 0 24px 60px -18px rgba(0, 0, 0, 0.8);
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
    color: var(--text-2);
  }
  .sizes {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
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
  .size:hover:not(:disabled) {
    color: var(--text-0);
    border-color: var(--line-strong);
  }
  .size.on {
    color: var(--on-accent);
    background: var(--accent);
    border-color: transparent;
    font-weight: 560;
  }
  .size:disabled {
    opacity: 0.38;
    cursor: not-allowed;
  }

  /* El borde discontinuo es la señal de "esto se arrastra"; al pasar por encima se vuelve sólido
     y se enciende el acento, igual que una zona de drop activa. */
  .drop {
    position: relative;
    display: block;
    width: 100%;
    aspect-ratio: 16 / 9;
    padding: 0;
    overflow: hidden;
    background: var(--bg-0);
    border: 2px dashed var(--line-strong);
    border-radius: var(--r-md);
    cursor: grab;
    transition: border-color 0.14s ease;
  }
  .drop:hover:not(.busy) {
    border-style: solid;
    border-color: var(--accent);
  }
  .drop:active:not(.busy) {
    cursor: grabbing;
  }
  .drop.busy {
    cursor: progress;
  }

  .media {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    pointer-events: none;
  }
  .media.hide {
    opacity: 0;
  }
  .vid {
    opacity: 0;
    transition: opacity 0.18s ease;
  }
  .vid.show {
    opacity: 1;
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
  .veil.hint {
    opacity: 0;
    transition: opacity 0.14s ease;
  }
  .drop:hover .veil.hint {
    opacity: 1;
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

  .error {
    margin-top: 12px;
    font-size: 12px;
    line-height: 1.4;
    color: #ff6166;
  }

  @media (prefers-reduced-motion: reduce) {
    .vid,
    .veil.hint,
    .fill {
      transition: none;
    }
  }
</style>
