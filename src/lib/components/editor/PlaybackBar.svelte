<script lang="ts">
  import { onDestroy } from 'svelte';
  import { editorState } from '$lib/editor-state.svelte';
  import { formatTimecode } from '$lib/timeline-math';
  import { t } from '$lib/i18n.svelte';
  import { playback } from './playback.svelte';
  import { ui } from './ui.svelte';

  let { fullscreen = false, surface = null, container = null }: {
    fullscreen?: boolean;
    surface?: HTMLElement | null;
    container?: HTMLElement | null;
  } = $props();
  let rail = $state<HTMLDivElement | null>(null);
  let pointer = $state<number | null>(null);
  let hovering = $state<number | null>(null);
  let overSurface = $state(false);
  const fraction = $derived(playback.kept > 0 ? Math.max(0, Math.min(1, playback.outPos / playback.kept)) : 0);
  const available = $derived(playback.kept > 0);
  let bounds = $state<{ left: number; top: number; width: number } | null>(null);

  // La barra acompaña al borde del vídeo (o del lienzo vertical), también al redimensionar
  // el visor o abrir paneles. Solo se mide al cambiar el layout, nunca por fotograma.
  $effect(() => {
    if (fullscreen || !surface || !container) { bounds = null; overSurface = false; return; }
    const target = surface;
    const host = container;
    let box = target.getBoundingClientRect();
    let mouse: { x: number; y: number } | null = null;
    const hit = () => {
      overSurface = !!mouse && box.width > 0 && box.height > 0 &&
        mouse.x >= box.left && mouse.x <= box.right && mouse.y >= box.top && mouse.y <= box.bottom;
    };
    const move = (event: PointerEvent) => {
      if (event.pointerType === 'touch') return;
      mouse = { x: event.clientX, y: event.clientY };
      hit();
    };
    const leave = () => { mouse = null; overSurface = false; };
    const measure = () => {
      box = target.getBoundingClientRect();
      const outer = host.getBoundingClientRect();
      bounds = box.width > 0 && box.height > 0
        ? { left: box.left - outer.left + 12, top: box.bottom - outer.top - 38, width: Math.max(0, box.width - 24) }
        : null;
      hit();
    };
    const observer = new ResizeObserver(measure);
    observer.observe(target);
    observer.observe(host);
    host.addEventListener('pointerenter', move, { passive: true });
    host.addEventListener('pointermove', move, { passive: true });
    host.addEventListener('pointerleave', leave);
    window.addEventListener('blur', leave);
    measure();
    return () => {
      observer.disconnect();
      host.removeEventListener('pointerenter', move);
      host.removeEventListener('pointermove', move);
      host.removeEventListener('pointerleave', leave);
      window.removeEventListener('blur', leave);
    };
  });

  function at(clientX: number) {
    if (!rail) return 0;
    const rect = rail.getBoundingClientRect();
    return Math.max(0, Math.min(1, (clientX - rect.left) / Math.max(1, rect.width)));
  }

  function showControls() {
    if (fullscreen) ui.showFsCtrl();
  }

  function down(event: PointerEvent) {
    if (event.button !== 0 || pointer !== null || !available || !rail) return;
    event.preventDefault();
    rail.focus({ preventScroll: true });
    rail.setPointerCapture(event.pointerId);
    pointer = event.pointerId;
    playback.pauseForSeek();
    hovering = at(event.clientX);
    playback.seekOutput(hovering * playback.kept);
    showControls();
  }

  function move(event: PointerEvent) {
    if (pointer !== null && pointer !== event.pointerId) return;
    hovering = at(event.clientX);
    if (pointer !== null) playback.seekOutput(hovering * playback.kept);
    showControls();
  }

  function end() {
    if (pointer === null) return;
    pointer = null;
    hovering = null;
    playback.resumeAfterGesture();
  }

  function up(event: PointerEvent) {
    if (pointer !== event.pointerId) return;
    playback.seekOutput(at(event.clientX) * playback.kept);
    end();
  }

  function key(event: KeyboardEvent) {
    if (!available) return;
    const step = event.shiftKey ? 1000 : 1000 / (editorState.fps || 30);
    let target = playback.outPos;
    if (event.key === 'ArrowLeft' || event.key === 'ArrowDown') target -= step;
    else if (event.key === 'ArrowRight' || event.key === 'ArrowUp') target += step;
    else if (event.key === 'PageDown') target -= 10_000;
    else if (event.key === 'PageUp') target += 10_000;
    else if (event.key === 'Home') target = 0;
    else if (event.key === 'End') target = playback.kept;
    else return;
    event.preventDefault();
    event.stopPropagation();
    playback.seekOutput(target);
    showControls();
  }

  onDestroy(end);
</script>

<div class="bar" class:fullscreen class:overlay={bounds !== null} class:shown={fullscreen || overSurface || pointer !== null}
  style:left={bounds ? `${bounds.left}px` : null}
  style:top={bounds ? `${bounds.top}px` : null}
  style:width={bounds ? `${bounds.width}px` : null}
  style:right={bounds ? 'auto' : null}
  style:bottom={bounds ? 'auto' : null}
>
  {#if fullscreen}<span class="time mono">{formatTimecode(playback.outPos)}</span>{/if}
  <div
    class="rail"
    class:active={pointer !== null || hovering !== null}
    class:disabled={!available}
    bind:this={rail}
    role="slider"
    tabindex={available ? 0 : -1}
    aria-label={t('ed.seek')}
    aria-disabled={!available}
    aria-valuemin={0}
    aria-valuemax={Math.round(playback.kept)}
    aria-valuenow={Math.round(playback.outPos)}
    aria-valuetext={`${formatTimecode(playback.outPos)} / ${formatTimecode(playback.kept)}`}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={end}
    onlostpointercapture={end}
    onpointerleave={() => { if (pointer === null) hovering = null; }}
    onkeydown={key}
  >
    <div class="track">
      <div class="fill" style:width="{fraction * 100}%"></div>
      <div class="knob" style:left="{fraction * 100}%"></div>
    </div>
    {#if fullscreen && available && hovering !== null}
      <span class="hover-time mono" style:left="{hovering * 100}%" style:translate="{-hovering * 100}% 0">
        {formatTimecode(hovering * playback.kept)}
      </span>
    {/if}
  </div>
  {#if fullscreen}<span class="time end mono">{formatTimecode(playback.kept)}</span>{/if}
</div>

<style>
  .bar {
    position: absolute; left: 18px; right: 18px; bottom: 8px; z-index: 4;
    display: flex; align-items: center; gap: 12px; height: 28px;
    opacity: 0; pointer-events: none; translate: 0 3px;
    transition: opacity 140ms ease, translate 160ms ease;
  }
  .bar.shown, .bar:has(.rail:focus-visible) { opacity: 1; pointer-events: auto; translate: 0 0; }
  .bar.fullscreen { position: fixed; left: 40px; right: 40px; bottom: 22px; z-index: 10000; }
  .bar.overlay { padding: 0 10px; border-radius: var(--r-sm); background: rgba(12,12,14,0.72); }
  .time { min-width: 58px; color: var(--text-0); font-size: 11px; text-shadow: 0 1px 3px #000; font-variant-numeric: tabular-nums; }
  .fullscreen .time { min-width: 64px; font-size: 12px; }
  .time.end { text-align: right; }
  .rail { position: relative; flex: 1; height: 24px; display: flex; align-items: center; cursor: pointer; touch-action: none; }
  .rail.disabled { opacity: 0.4; cursor: default; }
  .track { position: relative; width: 100%; height: 4px; border-radius: 999px; background: rgba(255,255,255,0.22); transition: height 150ms ease; }
  .rail.active .track, .rail:focus-visible .track { height: 6px; }
  .fill { height: 100%; border-radius: inherit; background: var(--accent); }
  .knob {
    position: absolute; top: 50%; width: 18px; height: 11px; border-radius: 3px;
    background: var(--accent-soft); box-shadow: 0 1px 4px rgba(0,0,0,0.6);
    transform: translate(-50%,-50%) scale(0); transition: transform 150ms ease; pointer-events: none;
  }
  .rail.active .knob, .rail:focus-visible .knob { transform: translate(-50%,-50%) scale(1); }
  .hover-time {
    position: absolute; bottom: calc(100% + 4px); padding: 4px 8px; font-size: 11.5px;
    color: var(--text-0); white-space: nowrap; background: var(--glass);
    border: 1px solid var(--line); border-radius: var(--r-sm); pointer-events: none;
  }
  @media (prefers-reduced-motion: reduce) { .bar, .track, .knob { transition: none; } }
</style>
