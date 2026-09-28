<script lang="ts">
  import Icon from '../Icon.svelte';
  import ValueInput from './ValueInput.svelte';
  import BlockLane from './BlockLane.svelte';
  import WaveTiles from './WaveTiles.svelte';
  import { beginGesture, commit, editorState, endGesture, preview, type AudioLane } from '$lib/editor-state.svelte';
  import { segLen, trackMix, withTrackMix, type TrackMix } from '$lib/edit-model';
  import { t } from '$lib/i18n.svelte';

  let {
    lane,
    mpp,
    width,
    viewX,
    viewW,
    posAt,
    headPos,
  }: {
    // null mientras se prepara el audio o si el clip no tiene.
    lane: AudioLane | null;
    mpp: number;
    width: number;
    viewX: number;
    viewW: number;
    posAt: (clientX: number) => number;
    headPos: number;
  } = $props();

  const id = $derived(lane?.id ?? 'sys');
  const kind = $derived(id === 'mic' ? 'mic' : id === 'game' ? 'game' : id.startsWith('app:') ? 'app' : 'sys');
  const mix = $derived(trackMix(editorState.edit.mixer, id));
  const vol = $derived(mix.vol);
  const muted = $derived(mix.muted);
  // Una pista sin WAV propio es el audio único del clip, que suena desde el vídeo.
  const embedded = $derived(!!lane && !lane.src);
  const label = $derived.by(() => {
    if (kind === 'mic') return t('ed.micAudio');
    if (kind === 'game') return lane?.name && lane.name !== 'Game' ? lane.name : t('ed.gameAudio');
    if (kind === 'app') return lane?.name || id.slice(4);
    return embedded || !lane ? t('ed.audio') : t('ed.sysAudio');
  });
  const icon = $derived(
    { mic: 'mic', game: 'gamepad', app: 'app', sys: embedded || !lane ? 'speaker' : 'headphones' }[kind],
  );
  const peaks = $derived(lane?.peaks ?? null);
  const noAudio = $derived(!lane && !editorState.loading);

  function withMixer(patch: Partial<TrackMix>) {
    return { ...editorState.edit, mixer: withTrackMix(editorState.edit.mixer, id, patch) };
  }
  const volPatch = (v: number): Partial<TrackMix> => ({ vol: v });

  function toggleMute() {
    commit(withMixer({ muted: !muted }));
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

  // Volumen escrito a mano (clic derecho en el porcentaje o en el tirador).
  let editing = $state(false);

  function editVolume(e: MouseEvent) {
    e.preventDefault();
    editing = true;
  }

  function typedVolume(v: number | null) {
    editing = false;
    if (v !== null && v !== Math.round(vol * 100)) commit(withMixer(volPatch(v / 100)));
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

  const px = (ms: number) => (mpp > 0 ? ms / mpp : 0);
</script>

<div class="row" class:muted>
  <div class="head">
    {#if editorState.loading}
      <div class="sk-ico"></div>
      <div class="sk-line"></div>
      <div class="sk-rail"></div>
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
        {#if lane?.icon}
          <img class="lane-ico" src={lane.icon} alt="" />
        {:else}
          <Icon name={icon} size={17} />
        {/if}
      </button>
      <span class="name">{label}</span>
      <span class="val">
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span class="pct mono" class:active={dragging || hovering} class:hidden={editing} oncontextmenu={editVolume}
          >{Math.round(vol * 100)}%</span
        >
        {#if editing}
          <ValueInput value={Math.round(vol * 100)} min={0} max={100} label={label} center width={40} ondone={typedVolume} />
        {/if}
      </span>
      <div
          class="rail"
          class:dragging
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
          oncontextmenu={editVolume}
        >
        <div class="bar"><div class="fill"></div></div>
        <div class="thumb"></div>
      </div>
    {/if}
  </div>
  <BlockLane track={id} {mpp} {width} {posAt} {headPos}>
    {#snippet block(s)}
      <WaveTiles
        {peaks}
        startMs={s.startMs}
        left={px(s.posMs)}
        width={px(segLen(s))}
        {mpp}
        {viewX}
        {viewW}
        {vol}
        dim={muted}
      />
    {/snippet}
    {#if noAudio}
      <span class="note mono">{t('ed.noAudio')}</span>
    {:else if !peaks}
      <div class="skeleton"></div>
    {/if}
  </BlockLane>
</div>

<style>
  /* Igual que la regla: las cabeceras de audio forman un solo bloque y la línea queda en el carril. */
  .row {
    position: relative;
    display: flex;
    height: 59px;
  }
  .row::after {
    content: '';
    position: absolute;
    left: var(--gutter);
    right: 0;
    bottom: 0;
    height: 1px;
    background: var(--line);
  }
  .head {
    position: sticky;
    left: 0;
    z-index: 3;
    flex: none;
    width: var(--gutter);
    /* Dos columnas: icono sobre porcentaje y nombre sobre tirador, alineados fila a fila. */
    display: grid;
    grid-template-columns: 32px minmax(0, 1fr);
    grid-template-rows: 22px 16px;
    align-content: center;
    align-items: center;
    column-gap: 10px;
    row-gap: 2px;
    padding: 0 14px 0 8px;
    background: var(--base);
    border-right: 1px solid var(--line);
  }
  .mute {
    width: 32px;
    height: 22px;
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
  .lane-ico {
    width: 18px;
    height: 18px;
    object-fit: contain;
    border-radius: 4px;
    transition: opacity 0.14s ease, filter 0.14s ease;
  }
  .mute.on .lane-ico {
    opacity: 0.4;
    filter: grayscale(1);
  }
  .name {
    min-width: 0;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pct {
    justify-self: center;
    font-size: 10.5px;
    line-height: 1;
    color: var(--text-3);
    transition: color 0.12s ease;
  }
  .pct.active {
    color: var(--text-1);
  }
  .val {
    position: relative;
    justify-self: center;
    display: inline-flex;
  }
  .pct.hidden {
    visibility: hidden;
  }
  .rail {
    position: relative;
    /* Más alta que su fila (16 px) a propósito: solo crece la zona de clic, la barra no cambia. */
    height: 20px;
    cursor: pointer;
    touch-action: none;
    outline: none;
  }
  .bar {
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    height: 6px;
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
    width: 20px;
    height: 14px;
    border-radius: 4px;
    background: var(--text-0);
    transform: translate(-50%, -50%);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.6);
    cursor: grab;
  }
  .thumb::after {
    content: '';
    position: absolute;
    top: 50%;
    left: 50%;
    width: 10px;
    height: 2px;
    border-radius: 1px;
    background: var(--bg-3);
    transform: translate(-50%, -50%);
  }
  .rail.dragging,
  .rail.dragging .thumb {
    cursor: grabbing;
  }
  .rail:focus-visible .thumb {
    box-shadow: 0 0 0 3px var(--accent-glow);
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
    top: 2px;
    height: 54px;
    border-radius: 6px;
  }
  .sk-ico {
    grid-row: 1 / 3;
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
