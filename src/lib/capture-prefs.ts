import { invoke } from '@tauri-apps/api/core';
import { replay } from './replay.svelte';
import { captureConfig, type QualityKey } from './capture-config.svelte';
import { hotkeys, hotkeyFailed } from './hotkeys.svelte';
import { replaySound, type SoundLevel } from './replay-sound.svelte';

// Los ajustes de captura viven en Rust (settings.json): el replay se arma sin interfaz, que se
// descarga con la ventana cerrada. Aquí solo se reflejan y se mandan enteros al cambiar uno.
type CapturePrefs = {
  replay: boolean;
  seconds: number;
  fps: number;
  quality: QualityKey;
  resolution: number;
  mic: boolean;
  micDevice: string;
  noiseSuppression: boolean;
  noiseLevel: number;
  sound: SoundLevel;
  hotkeys: { save: string; record: string; open: string };
};

let ready = false;

function current(): CapturePrefs {
  return {
    replay: replay.enabled,
    seconds: replay.seconds,
    fps: captureConfig.fps,
    quality: captureConfig.quality,
    resolution: captureConfig.resolution,
    mic: captureConfig.mic,
    micDevice: captureConfig.micDevice,
    noiseSuppression: captureConfig.noiseSuppression,
    noiseLevel: captureConfig.noiseLevel,
    sound: replaySound.level,
    hotkeys: { save: hotkeys.saveReplay, record: hotkeys.record, open: hotkeys.open }
  };
}

function apply(p: CapturePrefs) {
  replay.enabled = p.replay;
  replay.seconds = p.seconds;
  Object.assign(captureConfig, {
    fps: p.fps,
    quality: p.quality,
    resolution: p.resolution,
    mic: p.mic,
    micDevice: p.micDevice,
    noiseSuppression: p.noiseSuppression,
    noiseLevel: p.noiseLevel
  });
  replaySound.level = p.sound;
  Object.assign(hotkeys, { saveReplay: p.hotkeys.save, record: p.hotkeys.record, open: p.hotkeys.open });
}

function markFailed(failed: string[]) {
  hotkeyFailed.saveReplay = failed.includes(hotkeys.saveReplay);
  hotkeyFailed.record = failed.includes(hotkeys.record);
  hotkeyFailed.open = failed.includes(hotkeys.open);
}

// Antes de pintar (load del layout), para que la barra no enseñe un instante los valores por
// defecto. Si Rust aún no tiene ajustes es una instalación anterior: los stores vienen del
// localStorage y se le pasan tal cual.
export async function initCapturePrefs() {
  try {
    const saved = await invoke<CapturePrefs | null>('get_capture_prefs');
    if (saved) {
      apply(saved);
      markFailed(await invoke<string[]>('hotkey_failures'));
    } else {
      markFailed(await invoke<string[]>('set_capture_prefs', { prefs: current() }));
    }
  } catch {
    // fuera de Tauri (preview en navegador)
  }
  ready = true;
}

export function pushCapturePrefs() {
  if (!ready) return;
  invoke<string[]>('set_capture_prefs', { prefs: current() })
    .then(markFailed)
    .catch((e) => console.error('set_capture_prefs', e));
}
