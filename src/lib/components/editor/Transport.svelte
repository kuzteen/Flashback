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
    <Icon name="skip-back" size={19} />
  </button>
  <button class="tb" aria-label={t('ed.prevFrame')} data-tip={t('ed.prevFrame')} onclick={() => playback.stepFrame(-1)}>
    <Icon name="step-back" size={19} />
  </button>
  <button
    class="play"
    aria-label={playback.playing ? t('ed.pause') : t('ed.play')}
    onclick={() => playback.toggle()}
  >
    <Icon name={playback.playing ? 'stop' : 'play'} size={18} />
  </button>
  <button class="tb" aria-label={t('ed.nextFrame')} data-tip={t('ed.nextFrame')} onclick={() => playback.stepFrame(1)}>
    <Icon name="step-fwd" size={19} />
  </button>
  <button class="tb" aria-label={t('ed.goEnd')} data-tip={t('ed.goEnd')} onclick={() => playback.seekOutput(playback.kept)}>
    <Icon name="skip-fwd" size={19} />
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
  <div class="float">
    {@render controls()}
    <span class="vsep"></span>
    {@render extras()}
  </div>
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
  .time {
    font-size: 12.5px;
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
  .play {
    width: 40px;
    height: 40px;
    margin: 0 6px;
    display: grid;
    place-items: center;
    color: var(--on-accent);
    background: var(--accent);
    border-radius: 50%;
    transition: background 0.14s ease, transform 0.12s ease;
  }
  .play:hover {
    background: var(--accent-soft);
  }
  .play:active {
    transform: scale(0.95);
  }
  .float {
    position: fixed;
    left: 50%;
    bottom: 58px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 12px;
    background: rgba(18, 18, 20, 0.72);
    backdrop-filter: blur(14px);
    border: 1px solid rgba(255, 255, 255, 0.09);
    border-radius: 16px;
    box-shadow: 0 16px 44px rgba(0, 0, 0, 0.55);
    z-index: 10000;
  }
  .vsep {
    width: 1px;
    height: 20px;
    margin: 0 6px;
    background: var(--line-strong);
  }
</style>
