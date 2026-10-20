import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { initialState } from '../../edit-model';
import { PlaybackController } from './playback-controller';

function deferred() {
  let resolve!: () => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<void>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function media() {
  return {
    currentTime: 0, paused: true, seeking: false, muted: false, volume: 1,
    play: vi.fn(function (this: { paused: boolean }) { this.paused = false; return Promise.resolve(); }),
    pause: vi.fn(function (this: { paused: boolean }) { this.paused = true; }),
  };
}

describe('reproducción del editor', () => {
  let frames: Map<number, FrameRequestCallback>;
  let nextFrame: number;
  let video: ReturnType<typeof media>;
  let audio: ReturnType<typeof media>;
  let editor: { edit: ReturnType<typeof initialState>; tracks: { id: string; src: string | null }[]; fps: number; frameTimes: number[] };
  let player: PlaybackController;

  function frame() {
    const callbacks = [...frames.values()]; frames.clear();
    for (const callback of callbacks) callback(0);
  }

  beforeEach(() => {
    frames = new Map(); nextFrame = 0;
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { frames.set(++nextFrame, callback); return nextFrame; });
    vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id));
    video = media(); audio = media();
    editor = { edit: initialState(10_000), tracks: [{ id: 'sys', src: 'audio.wav' }], fps: 30, frameTimes: [] };
    player = new PlaybackController(() => editor);
    player.attach(video as unknown as HTMLVideoElement, { sys: audio as unknown as HTMLAudioElement });
  });

  afterEach(() => { player.reset(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

  it('un Play que termina después de pausar no revive el vídeo ni el audio', async () => {
    const pending = deferred(); video.play.mockReturnValueOnce(pending.promise);
    const start = player.play(); player.pause(); pending.resolve(); await start;
    expect(player.playing).toBe(false);
    expect(audio.play).not.toHaveBeenCalled();
    expect(frames.size).toBe(0);
  });

  it('el botón cancela Play aunque el navegador todavía no lo haya resuelto', async () => {
    const pending = deferred(); video.play.mockReturnValueOnce(pending.promise);
    const start = player.play(); player.toggle(); pending.resolve(); await start;
    expect(video.play).toHaveBeenCalledTimes(1);
    expect(player.playing).toBe(false);
    expect(video.paused).toBe(true);
  });

  it('no confunde el fotograma viejo con el final mientras busca rápidamente al inicio', async () => {
    await player.play(); player.seekOutput(0);
    video.seeking = true; video.currentTime = 10; frame();
    expect(player.playing).toBe(true);
    expect(player.outPos).toBe(0);
    video.seeking = false; video.currentTime = 0.1; frame();
    expect(player.outPos).toBe(100);
  });

  it('reset invalida Play pendiente antes de abrir otro clip', async () => {
    const pending = deferred(); video.play.mockReturnValueOnce(pending.promise);
    const start = player.play(); player.reset(); pending.resolve(); await start;
    expect(player.playing).toBe(false);
    expect(frames.size).toBe(0);
    expect(audio.play).not.toHaveBeenCalled();
  });

  it('conserva la intención de reproducir al arrastrar mientras Play está pendiente', async () => {
    const pending = deferred(); video.play.mockReturnValueOnce(pending.promise);
    const start = player.play(); player.pauseForSeek(); player.seekOutput(2000);
    pending.resolve(); await start;
    expect(player.playing).toBe(false);
    player.resumeAfterGesture(); await Promise.resolve();
    expect(player.playing).toBe(true);
    expect(player.outPos).toBe(2000);
  });

  it('buscar en pausa no inicia la reproducción al soltar', () => {
    player.pauseForSeek(); player.seekOutput(500); player.resumeAfterGesture();
    expect(player.outPos).toBe(500);
    expect(video.play).not.toHaveBeenCalled();
  });

  it('vuelve a reproducir desde cero después del final natural', async () => {
    await player.play(); video.currentTime = 10; frame();
    expect(player.playing).toBe(false);
    expect(player.outPos).toBe(10_000);
    await player.play();
    expect(player.playing).toBe(true);
    expect(video.currentTime).toBe(0);
  });

  it('la barra usa el tiempo editado al saltar sobre un recorte', async () => {
    const whole = editor.edit.segments[0];
    editor.edit.segments = [{ ...whole, endMs: 2000 }, { ...whole, startMs: 6000, posMs: 2000 }];
    player.seekOutput(3000);
    expect(video.currentTime).toBe(7);
    await player.play(); video.currentTime = 8; frame();
    expect(player.outPos).toBe(4000);
  });

  it('una petición antigua rechazada no pausa una reproducción más reciente', async () => {
    const pending = deferred(); video.play.mockReturnValueOnce(pending.promise);
    const first = player.play(); player.pause();
    await player.play();
    pending.reject(new DOMException('interrumpido', 'AbortError')); await first;
    expect(player.playing).toBe(true);
    expect(frames.size).toBe(1);
  });

  it('cambiar de vídeo invalida la reproducción pendiente del anterior', async () => {
    const pending = deferred(); video.play.mockReturnValueOnce(pending.promise);
    const first = player.play();
    const replacement = media();
    player.attach(replacement as unknown as HTMLVideoElement, {});
    pending.resolve(); await first;
    expect(player.playing).toBe(false);
    expect(audio.play).not.toHaveBeenCalled();
    expect(frames.size).toBe(0);
  });

  it('un fallo de Play deja el botón disponible para volver a intentarlo', async () => {
    const log = vi.spyOn(console, 'error').mockImplementation(() => {});
    video.play.mockRejectedValueOnce(new Error('decoder no preparado'));
    await player.play();
    expect(player.playing).toBe(false);
    expect(frames.size).toBe(0);
    await player.play();
    expect(player.playing).toBe(true);
    expect(log).toHaveBeenCalledTimes(1);
  });

  it('recupera un vídeo que el navegador dejó pausado tras buscar desde el final', async () => {
    await player.play();
    video.currentTime = 10; player.seekOutput(0);
    video.paused = true; video.seeking = true; frame();
    expect(video.play).toHaveBeenCalledTimes(1);
    video.currentTime = 0; video.seeking = false; frame();
    await Promise.resolve();
    expect(player.playing).toBe(true);
    expect(video.paused).toBe(false);
    expect(video.play).toHaveBeenCalledTimes(2);
    video.currentTime = 0.2; frame();
    expect(player.outPos).toBe(200);
  });

  it('no vuelve a buscar el mismo fotograma al reanudar', async () => {
    let source = 0;
    const seek = vi.fn((time: number) => { source = time; });
    Object.defineProperty(video, 'currentTime', { get: () => source, set: seek });
    player.seekOutput(2000); player.pause();
    seek.mockClear();
    await player.play();
    expect(seek).not.toHaveBeenCalled();
  });

  it('una pausa explícita durante un arrastre cancela la reanudación automática', async () => {
    await player.play(); player.pauseForSeek(); player.seekOutput(1000);
    player.pause(); player.resumeAfterGesture();
    expect(player.playing).toBe(false);
    expect(video.play).toHaveBeenCalledTimes(1);
  });
});
