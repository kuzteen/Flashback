<script lang="ts">
  import Icon from '../Icon.svelte';
  import { t } from '$lib/i18n.svelte';
  import { formatTimecode } from '$lib/timeline-math';
  import { playback } from './playback.svelte';
  import { ui } from './ui.svelte';

  // floating: la misma botonera flotando sobre el vídeo en pantalla completa.
  let { floating = false }: { floating?: boolean } = $props();
</script>

{#snippet controls()}
  <button class="tb" aria-label={t('ed.goStart')} data-tip={t('ed.goStart')} onclick={() => playback.seekOutput(0)}>
    <Icon name="skip-back" size={21} />
  </button>
  <button class="tb" aria-label={t('ed.prevFrame')} data-tip={t('ed.prevFrame')} onclick={() => playback.stepFrame(-1)}>
    <Icon name="step-back" size={19} />
  </button>
  <button
    class="play"
    aria-label={playback.playing ? t('ed.pause') : t('ed.play')}
    onclick={() => playback.toggle()}
  >
    <Icon name={playback.playing ? 'pause' : 'play-fill'} size={22} />
  </button>
  <button class="tb" aria-label={t('ed.nextFrame')} data-tip={t('ed.nextFrame')} onclick={() => playback.stepFrame(1)}>
    <Icon name="step-fwd" size={19} />
  </button>
  <button class="tb" aria-label={t('ed.goEnd')} data-tip={t('ed.goEnd')} onclick={() => playback.seekOutput(playback.kept)}>
    <Icon name="skip-fwd" size={21} />
  </button>
{/snippet}

{#snippet extras()}
  <button class="tb" aria-label={t('ed.captureFrame')} data-tip={t('ed.captureFrame')} onclick={() => ui.screenshot()}>
    <Icon name="camera" size={18} />
  </button>
  <button
    class="tb"
    aria-label={t('ed.fullscreen')}
    data-tip={t('ed.fullscreen')}
    data-tip-align="end"
    onclick={() => ui.setFs(!ui.fs)}
  >
    <Icon name="maximize" size={18} />
  </button>
{/snippet}

{#if floating}
  <!-- Dos paneles: la reproducción queda centrada en la pantalla y captura / pantalla completa
       aparte a la derecha, para que el play no se desplace del centro por los extras. -->
  <div class="float">{@render controls()}</div>
  <div class="float side">{@render extras()}</div>
{:else}
  <div class="transport">
    <span class="time mono">
      <span class="cur">{formatTimecode(playback.outPos)}</span>
      <span class="sep">/</span>
      <span class="dur">{formatTimecode(playback.kept)}</span>
    </span>
    <div class="center">{@render controls()}</div>
    <div class="right">{@render extras()}</div>
  </div>
{/if}

<style>
  .transport {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    height: 50px;
    padding: 0 16px;
    border-bottom: 1px solid var(--line);
  }
  /* Mismo recorte que la duración de las tarjetas: los dígitos no tienen descendente y el hueco
     que la fuente les reserva los dejaba por encima del centro de la barra. */
  .time {
    font-size: 12.5px;
    line-height: 1;
    text-box: trim-both cap alphabetic;
    color: var(--text-2);
  }
  .cur {
    color: var(--text-0);
  }
  .sep {
    margin: 0 6px;
    color: var(--text-3);
  }
  .center {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .right {
    display: flex;
    justify-content: flex-end;
    gap: 4px;
  }
  .tb {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    color: var(--text-2);
    border-radius: var(--r-sm);
    transition: color 0.14s ease, background 0.14s ease;
  }
  .tb:hover {
    color: var(--text-0);
    background: var(--bg-hover);
  }
  /* Sin círculo: el play es el control principal por tamaño y por color (blanco pleno frente al
     gris de los demás), no por un relleno que pesaba como el botón de exportar. */
  .play {
    width: 40px;
    height: 40px;
    margin: 0 4px;
    display: grid;
    place-items: center;
    color: var(--text-0);
    border-radius: var(--r-sm);
    transition: background 0.14s ease, transform 0.12s ease;
  }
  .play:hover {
    background: var(--bg-hover);
  }
  .play:active {
    transform: scale(0.95);
  }
  /* Paneles de cristal sobre el vídeo: las dos con el mismo alto para quedar alineados, y un
     brillo interior de 1 px arriba que les da volumen sin añadir color. */
  .float {
    position: fixed;
    left: 50%;
    bottom: 64px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 2px;
    height: 56px;
    padding: 0 8px;
    background: rgba(18, 18, 20, 0.62);
    backdrop-filter: blur(20px) saturate(140%);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.06),
      0 18px 48px -12px rgba(0, 0, 0, 0.6);
    z-index: 10000;
  }
  .float.side {
    left: auto;
    right: 40px;
    transform: none;
  }
  .float .tb {
    width: 40px;
    height: 40px;
    border-radius: var(--r-sm);
    color: var(--text-1);
  }
  .float .play {
    width: 46px;
    height: 46px;
    margin: 0 4px;
    border-radius: var(--r-md);
  }
  /* Sobre vídeo, el hover de la barra normal (5%) no se ve: aquí va más marcado. */
  .float .tb:hover,
  .float .play:hover {
    background: var(--line-strong);
  }
</style>
