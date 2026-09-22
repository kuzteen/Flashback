<script lang="ts">
  import { untrack } from 'svelte';
  import { editorState, setDuration } from '$lib/editor-state.svelte';
  import { t } from '$lib/i18n.svelte';
  import { playback } from './playback.svelte';
  import { ui } from './ui.svelte';
  import Transport from './Transport.svelte';

  let video = $state<HTMLVideoElement | null>(null);
  let sys = $state<HTMLAudioElement | null>(null);
  let mic = $state<HTMLAudioElement | null>(null);

  // attach aplica también el volumen: al leer la mezcla, el efecto se repite con cada cambio de
  // fader o silencio y el audio sigue a la edición sin más cableado.
  $effect(() => {
    playback.attach(video, sys, mic);
  });

  type VfcVideo = HTMLVideoElement & {
    requestVideoFrameCallback?: (cb: (now: number, meta: { mediaTime: number }) => void) => number;
    cancelVideoFrameCallback?: (handle: number) => void;
  };

  // rVFC dispara en cada fotograma pintado, también tras buscar o avanzar en pausa, así que
  // shownMediaTime es siempre el del fotograma visible.
  $effect(() => {
    const v = video as VfcVideo | null;
    if (!v || typeof v.requestVideoFrameCallback !== 'function') return;
    playback.vfc = true;
    let alive = true;
    let handle = 0;
    const cb = (_now: number, meta: { mediaTime: number }) => {
      playback.shownMediaTime = meta.mediaTime;
      if (alive) handle = v.requestVideoFrameCallback!(cb);
    };
    handle = v.requestVideoFrameCallback(cb);
    return () => {
      alive = false;
      v.cancelVideoFrameCallback?.(handle);
    };
  });

  // Arranca solo en cuanto el clip está listo (metadatos, pistas y montaje), una vez por clip.
  let autoPlayed: string | null = null;
  $effect(() => {
    const src = editorState.videoSrc;
    if (!src || editorState.loading || !video || playback.kept <= 0 || autoPlayed === src) return;
    autoPlayed = src;
    untrack(() => void playback.play());
  });

  function onLoaded() {
    setDuration((video?.duration || 0) * 1000);
    playback.onReady();
  }

  const frac = $derived(playback.kept > 0 ? Math.min(1, playback.outPos / playback.kept) : 0);
  let progEl = $state<HTMLDivElement | null>(null);
  let progDrag = false;

  function progAt(clientX: number) {
    if (!progEl) return;
    const r = progEl.getBoundingClientRect();
    const f = Math.max(0, Math.min(1, (clientX - r.left) / Math.max(1, r.width)));
    playback.seekOutput(f * playback.kept);
  }

  function onProgDown(e: PointerEvent) {
    if (e.button !== 0 || !progEl) return;
    e.preventDefault();
    progEl.setPointerCapture(e.pointerId);
    playback.pauseForSeek();
    progDrag = true;
    progAt(e.clientX);
  }

  function onProgMove(e: PointerEvent) {
    if (progDrag) progAt(e.clientX);
  }

  function onProgUp() {
    if (!progDrag) return;
    progDrag = false;
    playback.resumeAfterGesture();
  }
</script>

<div class="stage">
  {#if editorState.videoSrc}
    <div class="fit">
      <video
        bind:this={video}
        src={editorState.videoSrc}
        playsinline
        class:fs={ui.fs}
        class:nocursor={ui.fs && !ui.fsCtrlShow}
        onloadedmetadata={onLoaded}
        onended={() => playback.pause()}
        onclick={() => playback.toggle()}
      ><track kind="captions" /></video>
    </div>
  {/if}
  {#if editorState.system}
    <audio bind:this={sys} src={editorState.system} preload="auto"></audio>
  {/if}
  {#if editorState.mic}
    <audio bind:this={mic} src={editorState.mic} preload="auto"></audio>
  {/if}

  {#if editorState.loading}
    <div class="prep mono">{t('ed.preparingAudio')}</div>
  {:else if editorState.error}
    <div class="prep mono err">{t('ed.trackSplitError', { error: String(editorState.error) })}</div>
  {/if}
</div>

{#if ui.fs && ui.fsCtrlShow}
  <div
    class="fs-prog"
    bind:this={progEl}
    role="presentation"
    onpointerdown={onProgDown}
    onpointermove={onProgMove}
    onpointerup={onProgUp}
    onpointercancel={onProgUp}
  >
    <div class="track">
      <div class="fill" style:width="{frac * 100}%"></div>
      <div class="knob" style:left="{frac * 100}%"></div>
    </div>
  </div>
  <Transport floating />
{/if}

<style>
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    background: var(--base);
    overflow: hidden;
  }
  /* inset: 0 fija un alto definido para que el vídeo, con max-height: 100%, se reescale al
     cambiar el alto del panel inferior en vez de quedarse a tamaño fijo. */
  .fit {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 18px 40px;
  }
  video {
    max-width: 100%;
    max-height: 100%;
    display: block;
    cursor: pointer;
    border-radius: var(--r-md);
    box-shadow: 0 24px 60px -28px rgba(0, 0, 0, 0.9);
    object-fit: contain;
  }
  /* La ventana nativa ya cubre el monitor: el vídeo se fija al viewport entero y object-fit deja
     solo el letterbox real de su proporción. */
  video.fs {
    position: fixed;
    inset: 0;
    width: 100vw;
    height: 100vh;
    max-width: none;
    max-height: none;
    background: #000;
    border-radius: 0;
    box-shadow: none;
    z-index: 9999;
  }
  video.nocursor {
    cursor: none;
  }
  .prep {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    padding: 7px 12px;
    font-size: 12px;
    color: var(--text-1);
    background: var(--bg-2);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
  }
  .prep.err {
    color: var(--rec);
  }
  .fs-prog {
    position: fixed;
    left: 40px;
    right: 40px;
    bottom: 20px;
    height: 22px;
    display: flex;
    align-items: center;
    cursor: pointer;
    z-index: 10000;
  }
  .track {
    position: relative;
    width: 100%;
    height: 6px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.25);
  }
  .fill {
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
  }
  .knob {
    position: absolute;
    top: 50%;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent);
    transform: translate(-50%, -50%);
    pointer-events: none;
  }
</style>
