<script lang="ts">
  import Icon from '../Icon.svelte';
  import { beginGesture, commit, editorState, endGesture, preview } from '$lib/editor-state.svelte';
  import { segLen, type MixerState } from '$lib/edit-model';
  import { t } from '$lib/i18n.svelte';

  let {
    kind,
    mpp,
    width,
    viewW,
    scrollLeft,
  }: { kind: 'sys' | 'mic'; mpp: number; width: number; viewW: number; scrollLeft: number } = $props();


  const mixer = $derived(editorState.edit.mixer);
  const vol = $derived(kind === 'sys' ? mixer.sys_vol : mixer.mic_vol);
  const muted = $derived(kind === 'sys' ? mixer.sys_muted : mixer.mic_muted);
  // Sin pistas separadas, la de "sistema" es el audio único del clip.
  const label = $derived(
    kind === 'mic' ? t('ed.micAudio') : editorState.system ? t('ed.sysAudio') : t('ed.audio'),
  );
  const icon = $derived(kind === 'mic' ? 'mic' : editorState.system ? 'headphones' : 'speaker');
  const peaks = $derived(
    kind === 'mic' ? editorState.micPeaks : editorState.system ? editorState.sysPeaks : editorState.mixPeaks,
  );
  const noAudio = $derived(
    kind === 'sys' && !editorState.loading && !editorState.system && !editorState.mixPeaks,
  );

  function withMixer(patch: Partial<MixerState>) {
    return { ...editorState.edit, mixer: { ...editorState.edit.mixer, ...patch } };
  }
  const volPatch = (v: number): Partial<MixerState> => (kind === 'sys' ? { sys_vol: v } : { mic_vol: v });

  function toggleMute() {
    commit(withMixer(kind === 'sys' ? { sys_muted: !muted } : { mic_muted: !muted }));
  }

  let rail = $state<HTMLDivElement | null>(null);
  let dragging = $state(false);
  let hovering = $state(false);

  function volAt(clientX: number): number {
    if (!rail) return vol;
    const r = rail.getBoundingClientRect();
    return Math.max(0, Math.min(1, (clientX - r.left) / Math.max(1, r.width)));
  }

  function onRailDown(e: PointerEvent) {
    if (e.button !== 0 || !rail) return;
    e.preventDefault();
    rail.setPointerCapture(e.pointerId);
    beginGesture();
    dragging = true;
    preview(withMixer(volPatch(volAt(e.clientX))));
  }

  function onRailMove(e: PointerEvent) {
    if (dragging) preview(withMixer(volPatch(volAt(e.clientX))));
  }

  function onRailUp() {
    if (!dragging) return;
    dragging = false;
    endGesture();
  }

  function onRailKey(e: KeyboardEvent) {
    let v = vol;
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') v = Math.min(1, vol + 0.05);
    else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') v = Math.max(0, vol - 0.05);
    else if (e.key === 'Home') v = 0;
    else if (e.key === 'End') v = 1;
    else return;
    e.preventDefault();
    e.stopPropagation();
    commit(withMixer(volPatch(v)));
  }

  let canvas = $state<HTMLCanvasElement | null>(null);

  // Solo se pinta el tramo visible: con zoom alto un lienzo del ancho completo pasaría de los
  // 30.000 px. La capa base es la onda original sin editar, para que los huecos no queden
  // vacíos; los bloques se pintan encima (atenuados si están desactivados o la pista muda).
  function draw() {
    const c = canvas;
    if (!c) return;
    const w = Math.max(0, Math.floor(viewW));
    const h = c.clientHeight;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(w * dpr);
    c.height = Math.round(h * dpr);
    const ctx = c.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, w, h);
    const p = peaks;
    const dur = editorState.durationMs;
    if (!p || dur <= 0 || mpp <= 0 || w === 0) return;
    const mid = h / 2;
    const amp = h * 0.44;
    const bar = (src: number) => {
      const b = Math.min(p.length - 1, Math.max(0, Math.floor((src / dur) * p.length)));
      return Math.max(0.5, Math.min(1, p[b] * 1.8) * amp);
    };
    const ordered = [...editorState.edit.segments].sort((a, b) => a.posMs - b.posMs);
    const base = new Path2D();
    const on = new Path2D();
    const off = new Path2D();
    let k = 0;
    for (let x = 0; x < w; x++) {
      const pos = (scrollLeft + x) * mpp;
      if (pos < dur) {
        const y = bar(pos);
        base.rect(x, mid - y, 1, y * 2);
      }
      while (k < ordered.length && ordered[k].posMs + segLen(ordered[k]) <= pos) k++;
      const s = ordered[k];
      if (s && pos >= s.posMs) {
        const y = bar(s.startMs + (pos - s.posMs));
        (s.disabled || muted ? off : on).rect(x, mid - y, 1, y * 2);
      }
    }
    // El lienzo no entiende var(): los colores se leen de los tokens del tema al pintar.
    const css = getComputedStyle(c);
    ctx.fillStyle = css.getPropertyValue('--line').trim();
    ctx.fill(base);
    ctx.fillStyle = css.getPropertyValue('--line-strong').trim();
    ctx.fill(off);
    ctx.fillStyle = css.getPropertyValue('--text-0').trim();
    ctx.fill(on);
  }

  $effect(() => {
    void [canvas, peaks, editorState.edit.segments, editorState.durationMs, muted, mpp, viewW, scrollLeft];
    draw();
  });
</script>

<div class="row" class:muted>
  <div class="head">
    {#if editorState.loading}
      <div class="sk-ico"></div>
      <div class="info"><div class="sk-line"></div><div class="sk-rail"></div></div>
    {:else}
      <button
        class="mute"
        class:on={muted}
        aria-pressed={muted}
        aria-label={muted ? t('ed.unmute', { label }) : t('ed.mute', { label })}
        data-tip={muted ? t('ed.unmute', { label }) : t('ed.mute', { label })}
        data-tip-align="start"
        onclick={toggleMute}
      >
        <Icon name={muted ? 'speaker-off' : icon} size={17} />
      </button>
      <div class="info">
        <div class="line">
          <span class="name">{label}</span>
          <span class="pct mono" class:show={dragging || hovering}>{Math.round(vol * 100)}%</span>
        </div>
        <div
          class="rail"
          bind:this={rail}
          role="slider"
          tabindex="0"
          aria-label={label}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(vol * 100)}
          style:--v={vol}
          onpointerdown={onRailDown}
          onpointermove={onRailMove}
          onpointerup={onRailUp}
          onpointercancel={onRailUp}
          onpointerenter={() => (hovering = true)}
          onpointerleave={() => (hovering = false)}
          onkeydown={onRailKey}
        >
          <div class="bar"><div class="fill"></div></div>
          <div class="thumb"></div>
        </div>
      </div>
    {/if}
  </div>
  <div class="lane" style:width="{width}px">
    <canvas bind:this={canvas} class="wave" class:ready={!!peaks} style:width="{viewW}px"></canvas>
    {#if noAudio}
      <span class="note mono">{t('ed.noAudio')}</span>
    {:else if !peaks}
      <div class="skeleton"></div>
    {/if}
  </div>
</div>

<style>
  .row {
    display: flex;
    height: 52px;
    border-bottom: 1px solid var(--line);
  }
  .head {
    position: sticky;
    left: 0;
    z-index: 3;
    flex: none;
    width: var(--gutter);
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px 0 8px;
    background: var(--bg-0);
    border-right: 1px solid var(--line);
  }
  .mute {
    width: 32px;
    height: 32px;
    flex: none;
    display: grid;
    place-items: center;
    color: var(--text-1);
    border-radius: var(--r-sm);
    transition: color 0.14s ease, background 0.14s ease;
  }
  .mute:hover {
    color: var(--text-0);
    background: var(--bg-hover);
  }
  .mute.on {
    color: var(--rec);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .line {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 6px;
  }
  .name {
    min-width: 0;
    font-size: 12px;
    color: var(--text-1);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pct {
    font-size: 11px;
    color: var(--text-2);
    opacity: 0;
    transition: opacity 0.12s ease;
  }
  .pct.show {
    opacity: 1;
  }
  .rail {
    position: relative;
    height: 14px;
    cursor: pointer;
    touch-action: none;
    outline: none;
  }
  .bar {
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    height: 4px;
    transform: translateY(-50%);
    border-radius: 999px;
    background: var(--bg-3);
    overflow: hidden;
  }
  .fill {
    width: calc(var(--v) * 100%);
    height: 100%;
    background: var(--text-1);
  }
  .row.muted .fill {
    background: var(--text-3);
  }
  .thumb {
    position: absolute;
    top: 50%;
    left: calc(var(--v) * 100%);
    width: 16px;
    height: 10px;
    border-radius: 3px;
    background: var(--text-0);
    transform: translate(-50%, -50%);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.6);
  }
  .rail:focus-visible .thumb {
    box-shadow: 0 0 0 3px var(--accent-glow);
  }
  .lane {
    position: relative;
    flex: none;
  }
  /* Pegado al borde izquierdo visible: el lienzo mide lo que se ve y se repinta al hacer scroll. */
  .wave {
    position: sticky;
    left: var(--gutter);
    display: block;
    height: 100%;
    opacity: 0;
    transition: opacity 0.25s ease;
  }
  .wave.ready {
    opacity: 1;
  }
  .note {
    position: absolute;
    left: 12px;
    top: 50%;
    transform: translateY(-50%);
    font-size: 11px;
    color: var(--text-3);
  }
  .skeleton,
  .sk-ico,
  .sk-line,
  .sk-rail {
    background: linear-gradient(90deg, var(--bg-2), var(--bg-3), var(--bg-2));
    background-size: 200% 100%;
    animation: sk 1.2s ease-in-out infinite;
  }
  .skeleton {
    position: absolute;
    left: 0;
    right: 0;
    top: 14px;
    bottom: 14px;
    border-radius: 6px;
  }
  .sk-ico {
    width: 32px;
    height: 32px;
    border-radius: var(--r-sm);
  }
  .sk-line {
    width: 60%;
    height: 10px;
    border-radius: 4px;
  }
  .sk-rail {
    height: 4px;
    border-radius: 999px;
  }
  @keyframes sk {
    from {
      background-position: 100% 0;
    }
    to {
      background-position: -100% 0;
    }
  }
</style>
