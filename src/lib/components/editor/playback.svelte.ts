import { editorState } from '$lib/editor-state.svelte';
import { keptMs, outToSeg, trackMix } from '$lib/edit-model';
import { frameStepTarget, outStartOf } from '$lib/timeline-math';

// Dos bloques "pegados" en el origen (un corte que no quitó nada) no se buscan al pasar de uno a
// otro: fijar currentTime vacía el decoder y deja un microcorte. Solo se busca con salto real.
const SEAM_MS = 12;

class Playback {
  playing = $state(false);
  outPos = $state(0);
  // PTS del fotograma pintado según requestVideoFrameCallback: currentTime cae entre fotogramas
  // al pausar y la captura salía del de al lado.
  shownMediaTime = 0;
  vfc = false;

  private video: HTMLVideoElement | null = null;
  private audios: [string, HTMLAudioElement][] = [];
  private playIndex = 0;
  private raf = 0;
  private resumeAfterSeek = false;

  get kept(): number {
    return keptMs(editorState.edit.segments);
  }

  attach(video: HTMLVideoElement | null, audios: Record<string, HTMLAudioElement | null>) {
    this.video = video;
    this.audios = Object.entries(audios).filter((e): e is [string, HTMLAudioElement] => !!e[1]);
    this.applyAudio();
  }

  private get els(): HTMLAudioElement[] {
    return this.audios.map(([, a]) => a);
  }

  reset() {
    this.pause();
    this.outPos = 0;
    this.playIndex = 0;
    this.shownMediaTime = 0;
    this.vfc = false;
    this.resumeAfterSeek = false;
  }

  // Con pistas separadas suenan los <audio> y el vídeo va mudo; con pista única su fader controla
  // el audio del propio vídeo.
  applyAudio() {
    const m = editorState.edit.mixer;
    const lanes = editorState.tracks;
    if (this.video) {
      if (lanes.some((l) => l.src)) this.video.muted = true;
      else {
        const t = trackMix(m, lanes[0]?.id ?? 'sys');
        this.video.muted = t.muted;
        this.video.volume = t.vol;
      }
    }
    for (const [id, a] of this.audios) {
      const t = trackMix(m, id);
      a.volume = t.muted ? 0 : t.vol;
    }
  }

  private seekSource(srcMs: number) {
    if (!this.video) return;
    const t = srcMs / 1000;
    this.video.currentTime = t;
    for (const a of this.els) if (Math.abs(a.currentTime - t) > 0.05) a.currentTime = t;
  }

  seekOutput(T: number) {
    const segs = editorState.edit.segments;
    const t = Math.max(0, Math.min(T, this.kept));
    const { index, srcMs } = outToSeg(segs, t);
    this.playIndex = index;
    this.outPos = t;
    this.seekSource(srcMs);
  }

  // Muestra un tiempo de origen concreto (el borde que se recorta) sin pasar por el mapeo de
  // salida, que en el final de un bloque saltaría al principio del siguiente.
  peek(outMs: number, srcMs: number) {
    this.outPos = outMs;
    this.seekSource(srcMs);
  }

  // Tras cambiar el montaje, el cabezal se reengancha a un tiempo que siga existiendo.
  settle() {
    this.seekOutput(Math.min(this.outPos, this.kept));
  }

  onReady() {
    this.playIndex = 0;
    this.outPos = 0;
    this.seekSource(editorState.edit.segments[0]?.startMs ?? 0);
    this.applyAudio();
  }

  private tick = () => {
    const v = this.video;
    if (!v || !this.playing) return;
    const segs = editorState.edit.segments;
    const s = segs[this.playIndex];
    if (!s) {
      this.pause();
      return;
    }
    const srcMs = v.currentTime * 1000;
    for (const a of this.els) {
      if (Math.abs(a.currentTime - v.currentTime) > 0.12) a.currentTime = v.currentTime;
    }
    if (srcMs >= s.endMs - 1) {
      let ni = this.playIndex + 1;
      while (ni < segs.length && segs[ni].disabled) ni++;
      if (ni < segs.length) {
        const next = segs[ni];
        this.playIndex = ni;
        if (Math.abs(next.startMs - s.endMs) > SEAM_MS) this.seekSource(next.startMs);
        this.outPos = outStartOf(segs, ni);
      } else {
        this.pause();
        this.outPos = this.kept;
        return;
      }
    } else {
      this.outPos = outStartOf(segs, this.playIndex) + Math.max(0, srcMs - s.startMs);
    }
    this.raf = requestAnimationFrame(this.tick);
  };

  async play() {
    const v = this.video;
    if (!v || this.kept <= 0) return;
    this.seekOutput(this.outPos >= this.kept ? 0 : this.outPos);
    try {
      await v.play();
      for (const a of this.els) a.play().catch(() => {});
    } catch (e) {
      console.error('editor play', e);
      return;
    }
    this.playing = true;
    cancelAnimationFrame(this.raf);
    this.raf = requestAnimationFrame(this.tick);
  }

  pause() {
    this.video?.pause();
    for (const a of this.els) a.pause();
    this.playing = false;
    cancelAnimationFrame(this.raf);
  }

  toggle() {
    if (this.playing) this.pause();
    else void this.play();
  }

  // Mover el cabezal mientras suena dispara un blip de audio por fotograma: se pausa al empezar
  // a arrastrar y se reanuda al soltar si estaba sonando.
  pauseForSeek() {
    this.resumeAfterSeek = this.playing;
    if (this.playing) this.pause();
  }

  resumeAfterGesture() {
    if (!this.resumeAfterSeek) return;
    this.resumeAfterSeek = false;
    void this.play();
  }

  stepFrame(dir: 1 | -1) {
    const segs = editorState.edit.segments;
    const { index, srcMs } = outToSeg(segs, this.outPos);
    const s = segs[index];
    if (!s) return;
    const frameMs = 1000 / (editorState.fps || 30);
    const target = frameStepTarget(editorState.frameTimes, srcMs, dir, frameMs);
    if (target !== null && target >= s.startMs && target < s.endMs) {
      this.seekOutput(outStartOf(segs, index) + (target - s.startMs));
    } else {
      this.seekOutput(this.outPos + dir * frameMs);
    }
  }

  shownMs(): number {
    if (this.vfc) return this.shownMediaTime * 1000;
    return (this.video?.currentTime ?? 0) * 1000;
  }
}

export const playback = new Playback();
