<script lang="ts" module>
  export type SoundDraft = { name: string; durationMs: number; peaks: number[]; truncated: boolean };
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import Icon from '$lib/components/Icon.svelte';
  import { t } from '$lib/i18n.svelte';

  let { draft, onclose }: { draft: SoundDraft; onclose: (name: string | null) => void } = $props();

  const MAX_MS = 2000;
  // Por debajo no se oye un aviso, solo un clic.
  const MIN_MS = 200;
  const maxLen = $derived(Math.min(MAX_MS, draft.durationMs));
  const minLen = $derived(Math.min(MIN_MS, draft.durationMs));

  let start = $state(0);
  // El modal se monta de nuevo para cada archivo, así que la duración inicial sale una sola vez.
  let len = $state(untrack(() => Math.min(MAX_MS, draft.durationMs)));
  const resizable = $derived(maxLen > minLen);
  const movable = $derived(draft.durationMs > len);

  let wave = $state<HTMLDivElement | null>(null);
  let card = $state<HTMLDivElement | null>(null);
  type Drag = { kind: 'move'; grab: number } | { kind: 'start' } | { kind: 'end' };
  let drag: Drag | null = null;
  let playPos = $state<number | null>(null);
  let raf = 0;
  let busy = $state(false);

  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
  const firstBar = $derived(Math.floor((start / draft.durationMs) * draft.peaks.length));
  const lastBar = $derived(Math.ceil(((start + len) / draft.durationMs) * draft.peaks.length));

  function moveTo(ms: number) {
    start = clamp(ms, 0, draft.durationMs - len);
  }

  function setStartEdge(ms: number) {
    const end = start + len;
    const s = clamp(ms, Math.max(0, end - maxLen), end - minLen);
    start = s;
    len = end - s;
  }

  function setEndEdge(ms: number) {
    len = clamp(ms, start + minLen, Math.min(draft.durationMs, start + maxLen)) - start;
  }

  function fmt(ms: number) {
    const s = ms / 1000;
    return `${Math.floor(s / 60)}:${(s % 60).toFixed(1).padStart(4, '0')}`;
  }

  function msAt(clientX: number) {
    const r = wave!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * draft.durationMs;
  }

  function preview() {
    cancelAnimationFrame(raf);
    invoke('preview_save_sound', { startMs: Math.round(start), lenMs: Math.round(len) }).catch(() => {});
    const t0 = performance.now();
    const tick = (now: number) => {
      const p = (now - t0) / len;
      playPos = p < 1 ? p : null;
      if (p < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
  }

  function onDown(e: PointerEvent) {
    if (e.button !== 0) return;
    const edge = (e.target as HTMLElement).closest<HTMLElement>('[data-edge]')?.dataset.edge;
    if (edge === 'start' || edge === 'end') {
      drag = { kind: edge };
    } else {
      if (!movable) return;
      const at = msAt(e.clientX);
      if (at >= start && at <= start + len) {
        drag = { kind: 'move', grab: at - start };
      } else {
        drag = { kind: 'move', grab: len / 2 };
        moveTo(at - len / 2);
      }
    }
    wave!.setPointerCapture(e.pointerId);
    cancelAnimationFrame(raf);
    playPos = null;
  }

  function onMove(e: PointerEvent) {
    if (!drag) return;
    const at = msAt(e.clientX);
    if (drag.kind === 'move') moveTo(at - drag.grab);
    else if (drag.kind === 'start') setStartEdge(at);
    else setEndEdge(at);
  }

  // Al soltar se escucha el tramo: así se elige de oído, sin pulsar Escuchar en cada intento.
  function onUp() {
    if (!drag) return;
    drag = null;
    preview();
  }

  function onKey(e: KeyboardEvent, edge?: 'start' | 'end') {
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault();
      e.stopPropagation();
      const step = (e.shiftKey ? 1000 : 100) * (e.key === 'ArrowLeft' ? -1 : 1);
      if (edge === 'start') setStartEdge(start + step);
      else if (edge === 'end') setEndEdge(start + len + step);
      else moveTo(start + step);
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      e.stopPropagation();
      preview();
    }
  }

  function cancel() {
    if (busy) return;
    cancelAnimationFrame(raf);
    invoke('discard_save_sound').catch(() => {});
    onclose(null);
  }

  async function accept() {
    if (busy) return;
    busy = true;
    cancelAnimationFrame(raf);
    try {
      onclose(await invoke<string>('accept_save_sound', { startMs: Math.round(start), lenMs: Math.round(len) }));
    } catch (e) {
      console.error('accept_save_sound', e);
      busy = false;
    }
  }

  function onWindowKey(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    e.preventDefault();
    e.stopImmediatePropagation();
    cancel();
  }

  $effect(() => {
    card?.focus({ preventScroll: true });
    return () => cancelAnimationFrame(raf);
  });
</script>

<svelte:window onkeydown={onWindowKey} />

<div
  class="backdrop"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) cancel();
  }}
>
  <div class="card" role="dialog" aria-modal="true" aria-labelledby="trim-title" tabindex="-1" bind:this={card}>
    <h2 class="modal-title" id="trim-title">{t('sound.trim.title')}</h2>
    <p class="file mono" title={draft.name}>{draft.name}</p>

    <div
      class="wave"
      class:movable
      bind:this={wave}
      role="presentation"
      onpointerdown={onDown}
      onpointermove={onMove}
      onpointerup={onUp}
      onpointercancel={onUp}
    >
      <svg viewBox="0 0 {draft.peaks.length} 100" preserveAspectRatio="none" aria-hidden="true">
        {#each draft.peaks as p, i (i)}
          {@const h = Math.max(2, p * 92)}
          <rect x={i + 0.18} y={50 - h / 2} width="0.64" height={h} class:in={i >= firstBar && i < lastBar} />
        {/each}
      </svg>
      <div
        class="sel"
        role="slider"
        tabindex="0"
        aria-label={t('sound.trim.selection')}
        aria-valuemin={0}
        aria-valuemax={Math.round(draft.durationMs - len)}
        aria-valuenow={Math.round(start)}
        aria-valuetext="{fmt(start)} – {fmt(start + len)}"
        style:left="{(start / draft.durationMs) * 100}%"
        style:width="{(len / draft.durationMs) * 100}%"
        onkeydown={(e) => onKey(e)}
      >
        {#if playPos !== null}<span class="head" style:left="{playPos * 100}%"></span>{/if}
        {#if resizable}
          {#each ['start', 'end'] as const as edge (edge)}
            <span
              class="edge {edge}"
              data-edge={edge}
              role="slider"
              tabindex="0"
              aria-label={t(edge === 'start' ? 'sound.trim.startEdge' : 'sound.trim.endEdge')}
              aria-valuemin={0}
              aria-valuemax={Math.round(draft.durationMs)}
              aria-valuenow={Math.round(edge === 'start' ? start : start + len)}
              aria-valuetext={fmt(edge === 'start' ? start : start + len)}
              onkeydown={(e) => onKey(e, edge)}
            ><span class="grip"></span></span>
          {/each}
        {/if}
      </div>
    </div>

    <div class="meta">
      <button class="listen" onclick={preview}>
        <Icon name="play-fill" size={13} /><span class="txt">{t('sound.trim.listen')}</span>
      </button>
      <span class="range mono">{fmt(start)} – {fmt(start + len)} · {(len / 1000).toFixed(1)} s</span>
      <span class="total mono">{fmt(draft.durationMs)}</span>
    </div>

    <p class="hint">
      {resizable ? t('sound.trim.hint') : t('sound.trim.short')}
      {#if draft.truncated}{t('sound.trim.truncated')}{/if}
    </p>

    <div class="actions">
      <button class="btn" onclick={cancel}>{t('confirm.cancel')}</button>
      <button class="btn primary" disabled={busy} onclick={accept}>{t('sound.trim.use')}</button>
    </div>
  </div>
</div>

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
    width: 560px;
    max-width: calc(100vw - 40px);
    padding: 20px 22px 18px;
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-dialog);
    text-align: center;
    animation: dlg-in 0.16s ease-out;
  }
  .card:focus {
    outline: none;
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

  .file {
    margin-top: 8px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
    color: var(--text-2);
  }

  .wave {
    position: relative;
    height: 104px;
    margin-top: 18px;
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    touch-action: none;
    user-select: none;
  }
  .wave.movable {
    cursor: pointer;
  }
  svg {
    position: absolute;
    inset: 8px 0;
    width: 100%;
    height: calc(100% - 16px);
  }
  rect {
    fill: var(--wave-off);
  }
  rect.in {
    fill: var(--wave);
  }

  .sel {
    position: absolute;
    top: -1px;
    bottom: -1px;
    border: 1px solid var(--accent);
    border-radius: var(--r-sm);
    background: color-mix(in srgb, var(--accent) 9%, transparent);
    outline: none;
  }
  .movable .sel {
    cursor: grab;
  }
  .movable .sel:active {
    cursor: grabbing;
  }
  .sel:focus-visible {
    box-shadow: 0 0 0 3px var(--accent-glow);
  }
  .edge {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 14px;
    display: grid;
    place-items: center;
    cursor: ew-resize;
    outline: none;
  }
  .edge.start {
    left: -7px;
  }
  .edge.end {
    right: -7px;
  }
  .grip {
    width: 4px;
    height: 22px;
    border-radius: 2px;
    background: var(--accent);
    transition: transform 0.12s ease;
  }
  .edge:hover .grip,
  .edge:focus-visible .grip {
    transform: scaleY(1.25);
  }
  .edge:focus-visible .grip {
    box-shadow: 0 0 0 3px var(--accent-glow);
  }
  .head {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--accent-soft);
    pointer-events: none;
  }

  .meta {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    margin-top: 10px;
  }
  .listen {
    justify-self: start;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px 0 8px;
    font-size: 12.5px;
    color: var(--text-1);
    border-radius: var(--r-sm);
    transition: color 0.14s ease, background 0.14s ease;
  }
  .listen:hover {
    color: var(--text-0);
    background: var(--bg-3);
  }
  .txt {
    line-height: 1;
    text-box: trim-both cap alphabetic;
  }
  .range {
    font-size: 12.5px;
    color: var(--text-0);
  }
  .total {
    justify-self: end;
    font-size: 12px;
    color: var(--text-3);
  }

  .hint {
    margin-top: 12px;
    font-size: 12px;
    line-height: 1.4;
    color: var(--text-2);
  }

  .actions {
    display: flex;
    justify-content: center;
    gap: 8px;
    margin-top: 20px;
  }
  /* Mismo botón que el resto de modales: alto y ancho fijos para que el primario no se lea mayor. */
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
