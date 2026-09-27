<script lang="ts">
  import { TOAST_IN, TOAST_OUT, toastSlide } from '$lib/toast-motion';
  import Icon from './Icon.svelte';
  import { exportTask } from '$lib/export-task.svelte';
  import { t } from '$lib/i18n.svelte';

  let {
    title,
    seq,
    duration = 5000,
    onundo,
    ondone
  }: { title: string; seq: number; duration?: number; onundo: () => void; ondone: () => void } =
    $props();

  // La cuenta atrás es la propia animación del anillo: se pausa con el ratón encima (quien va a
  // pulsar Deshacer no debe perderlo por un segundo) y el aviso se va cuando termina. Un `seq`
  // nuevo reinicia solo el anillo; el aviso se queda donde está.
  let hover = $state(false);
</script>

<div
  class="toast"
  class:below={exportTask.phase !== 'idle'}
  role="status"
  aria-live="polite"
  onmouseenter={() => (hover = true)}
  onmouseleave={() => (hover = false)}
  in:toastSlide={{ from: -1, duration: TOAST_IN }}
  out:toastSlide={{ from: -1, duration: TOAST_OUT }}
>
  <span class="icon">
    <svg width="22" height="22" viewBox="0 0 20 20" aria-hidden="true">
      <circle class="track" cx="10" cy="10" r="8" />
      {#key seq}
        <circle
          class="ring"
          class:paused={hover}
          cx="10"
          cy="10"
          r="8"
          pathLength="1"
          style:animation-duration="{duration}ms"
          onanimationend={ondone}
        />
      {/key}
    </svg>
  </span>
  <span class="title">{title}</span>
  <span class="actions">
    <button class="act" onclick={onundo}>{t('pl.undo')}</button>
    <button class="x" aria-label={t('exp.dismiss')} onclick={ondone}>
      <Icon name="close" size={14} sw={2.2} />
    </button>
  </span>
</div>

<style>
  .toast {
    position: fixed;
    top: 14px;
    left: 50%;
    z-index: 900;
    width: min(420px, calc(100vw - 32px));
    height: 64px;
    translate: -50% 0;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 0 10px 0 16px;
    background: var(--bg-0);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-lg);
    box-shadow: var(--shadow-pop);
  }
  /* Con el aviso de exportación a la vista, justo debajo de él en vez de encima. */
  .toast.below {
    top: 88px;
  }
  .icon {
    display: grid;
  }
  .icon svg {
    fill: none;
    stroke-width: 1.8;
    stroke-linecap: round;
  }
  .track {
    stroke: var(--line-strong);
  }
  .ring {
    stroke: var(--accent);
    stroke-dasharray: 1;
    transform: rotate(-90deg);
    transform-origin: center;
    animation-name: drain;
    animation-timing-function: linear;
    animation-fill-mode: forwards;
  }
  .ring.paused {
    animation-play-state: paused;
  }
  @keyframes drain {
    from {
      stroke-dashoffset: 0;
    }
    to {
      stroke-dashoffset: 1;
    }
  }
  .title {
    overflow: hidden;
    font-size: 13.5px;
    font-weight: 600;
    color: var(--text-0);
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
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
  .x {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    color: var(--text-2);
    border-radius: var(--r-sm);
    transition: color 0.12s ease, background 0.12s ease;
  }
  .x:hover {
    color: var(--text-0);
    background: var(--bg-hover);
  }
</style>
