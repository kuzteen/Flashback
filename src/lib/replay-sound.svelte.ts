import { invoke } from '@tauri-apps/api/core';
import { t } from './i18n.svelte';
import { pushCapturePrefs } from './capture-prefs';

// Se lee del localStorage solo para migrar a Rust la primera vez (ver capture-prefs.ts).

const LEVEL_KEY = 'flashback.replay.soundLevel';

export type SoundLevel = 'off' | 'low' | 'normal' | 'high';

// La ganancia de cada nivel la aplica Rust (CapturePrefs::sound_gain).
export const SOUND_OPTIONS: { key: SoundLevel }[] = [{ key: 'off' }, { key: 'low' }, { key: 'normal' }, { key: 'high' }];

function loadLevel(): SoundLevel {
  if (typeof localStorage === 'undefined') return 'normal';
  const v = localStorage.getItem(LEVEL_KEY);
  return SOUND_OPTIONS.some((o) => o.key === v) ? (v as SoundLevel) : 'normal';
}

export const replaySound = $state<{ level: SoundLevel }>({ level: loadLevel() });

export function setReplaySoundLevel(level: SoundLevel) {
  replaySound.level = level;
  pushCapturePrefs();
}

export function soundLabel(level: SoundLevel): string {
  return t(`sound.${level}`);
}

// Suena por el mismo camino que el atajo (Rust), así se oye el sonido elegido, personalizado incluido.
export function playReplaySound() {
  invoke('test_save_sound').catch(() => {});
}
