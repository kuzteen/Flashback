<script lang="ts">
  import { fade } from 'svelte/transition';
  import { TOAST_IN, TOAST_OUT, toastSlide } from '$lib/toast-motion';
  import Icon from './Icon.svelte';
  import { exportStage } from '$lib/export-stage';
  import {
    cancelExport,
    dismissExport,
    exportTask,
    retryExport,
    viewExported
  } from '$lib/export-task.svelte';
  import { t } from '$lib/i18n.svelte';

  const RING = 2 * Math.PI * 8;
  const phase = $derived(exportTask.phase);
  const pct = $derived(Math.round(exportTask.progress * 100));
  const fileName = $derived(exportTask.dst?.split(/[\\/]/).pop() ?? '');

  // El aviso de "listo" espera mientras el ratón está encima: quien va a pulsar Ver clip no debe
  // perderlo por un segundo.
  let hover = $state(false);
  $effect(() => {
    const ms = phase === 'done' ? 6000 : phase === 'cancelled' ? 3000 : 0;
    if (!ms || hover) return;
    const id = setTimeout(dismissExport, ms);
    return () => clearTimeout(id);
  });
</script>

{#if phase !== 'idle'}
  <div
    class="toast"
    class:failed={phase === 'failed'}
    role="status"
    aria-live="polite"
    onmouseenter={() => (hover = true)}
    onmouseleave={() => (hover = false)}
    in:toastSlide={{ from: -1, duration: TOAST_IN }}
    out:toastSlide={{ from: -1, duration: TOAST_OUT }}
  >
    {#key phase}
      <div class="body" in:fade={{ duration: 160 }}>
        <span class="icon">
          {#if phase === 'running'}
            <svg width="22" height="22" viewBox="0 0 20 20" aria-hidden="true">
              <circle class="track" cx="10" cy="10" r="8" />
              <circle
                class="ring"
                cx="10"
                cy="10"
                r="8"
                stroke-dasharray={RING}
                stroke-dashoffset={RING * (1 - exportTask.progress)}
              />
            </svg>
          {:else if phase === 'done'}
            <svg width="22" height="22" viewBox="0 0 20 20" aria-hidden="true">
              <circle class="ring full" cx="10" cy="10" r="8" />
              <path class="tick" d="M6.4 10.3l2.4 2.4 4.8-5" pathLength="1" />
            </svg>
          {:else if phase === 'failed'}
            <svg width="22" height="22" viewBox="0 0 20 20" aria-hidden="true">
              <circle class="ring alert" cx="10" cy="10" r="8" />
              <path class="mark" d="M10 6v4.6M10 13.6v.1" />
            </svg>
          {:else}
            <svg width="22" height="22" viewBox="0 0 20 20" aria-hidden="true">
              <circle class="track" cx="10" cy="10" r="8" />
              <path class="mark muted" d="M7.6 7.6l4.8 4.8M12.4 7.6l-4.8 4.8" />
            </svg>
          {/if}
        </span>

        <span class="text">
          {#if phase === 'running'}
            <span class="title">{t('exp.running')}</span>
            <span class="sub">
              {exportTask.cancelling ? t('ed.cancelling') : `${t(exportStage(exportTask.progress))}…`}
            </span>
          {:else if phase === 'done'}
            <span class="title">{t('exp.done')}</span>
            <span class="sub name">{fileName}</span>
          {:else if phase === 'failed'}
            <span class="title">{t('exp.failed')}</span>
            <span class="sub">{exportTask.error}</span>
          {:else}
            <span class="title">{t('ed.exportCancelled')}</span>
          {/if}
        </span>

        <span class="actions">
          {#if phase === 'running'}
            <span class="pct mono">{pct}%</span>
            <button
              class="x"
              aria-label={t('ed.cancelExport')}
              disabled={exportTask.cancelling}
              onclick={cancelExport}
            >
              <Icon name="close" size={14} sw={2.2} />
            </button>
          {:else}
            {#if phase === 'done'}
              <button class="act" onclick={viewExported}>{t('ed.viewClip')}</button>
            {:else if phase === 'failed'}
              <button class="act quiet" onclick={retryExport}>{t('ed.retry')}</button>
            {/if}
            <button class="x" aria-label={t('exp.dismiss')} onclick={dismissExport}>
              <Icon name="close" size={14} sw={2.2} />
            </button>
          {/if}
        </span>
      </div>
    {/key}
    {#if phase === 'running'}
      <span class="bar" style:--p={exportTask.progress} out:fade={{ duration: 160 }}></span>
    {/if}
  </div>
{/if}

<style>
  .toast {
    position: fixed;
    top: 14px;
    left: 50%;
    z-index: 900;
    width: min(420px, calc(100vw - 32px));
    height: 64px;
    translate: -50% 0;
    overflow: hidden;
    background: var(--bg-0);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-lg);
    box-shadow: var(--shadow-pop);
  }
  .body {
    position: absolute;
    inset: 0;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 0 10px 0 16px;
  }
  .icon {
    display: grid;
    color: var(--text-0);
  }
  .icon svg {
    fill: none;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .track {
    stroke: var(--line-strong);
  }
  .ring {
    stroke: var(--accent);
    transform: rotate(-90deg);
    transform-origin: center;
    transition: stroke-dashoffset 0.2s ease;
  }
  .ring.alert {
    stroke: var(--rec);
  }
  .tick {
    stroke: var(--accent);
    stroke-width: 2;
    stroke-dasharray: 1;
    stroke-dashoffset: 1;
    animation: draw 0.34s 0.12s cubic-bezier(0.23, 1, 0.32, 1) forwards;
  }
  @keyframes draw {
    to {
      stroke-dashoffset: 0;
    }
  }
  .mark {
    stroke: var(--rec-text);
    stroke-width: 2;
  }
  .mark.muted {
    stroke: var(--text-2);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .title {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--text-0);
  }
  .sub {
    overflow: hidden;
    font-size: 12.5px;
    color: var(--text-2);
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .pct {
    min-width: 4ch;
    font-size: 13px;
    font-weight: 560;
    font-variant-numeric: tabular-nums;
    text-align: right;
    color: var(--text-1);
  }
  .act {
    height: 30px;
    padding: 0 12px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--on-bright);
    background: var(--bright);
    border-radius: var(--r-sm);
    transition: background 0.14s ease;
  }
  .act:hover {
    background: var(--accent-soft);
  }
  .act.quiet {
    color: var(--text-0);
    background: var(--bg-2);
    border: 1px solid var(--line-strong);
  }
  .act.quiet:hover {
    background: var(--bg-3);
  }
  .x {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    color: var(--text-2);
    border-radius: var(--r-sm);
    transition: color 0.12s ease, background 0.12s ease;
  }
  .x:hover:not(:disabled) {
    color: var(--text-0);
    background: var(--bg-hover);
  }
  .x:disabled {
    opacity: 0.4;
  }
  .bar {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
    background: var(--accent);
    transform: scaleX(var(--p));
    transform-origin: left;
    transition: transform 0.2s ease;
  }
  @media (prefers-reduced-motion: reduce) {
    .tick {
      animation-duration: 1ms;
    }
  }
</style>
