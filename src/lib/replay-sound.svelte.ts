import { invoke } from '@tauri-apps/api/core';
import { t } from './i18n.svelte';

const LEVEL_KEY = 'flashback.replay.soundLevel';

export type SoundLevel = 'off' | 'low' | 'normal' | 'high';

export const SOUND_OPTIONS: { key: SoundLevel; gain: number }[] = [
  { key: 'off', gain: 0 },
  { key: 'low', gain: 0.25 },
  { key: 'normal', gain: 0.55 },
  { key: 'high', gain: 1.0 }
];

function loadLevel(): SoundLevel {
  if (typeof localStorage === 'undefined') return 'normal';
  const v = localStorage.getItem(LEVEL_KEY);
  return SOUND_OPTIONS.some((o) => o.key === v) ? (v as SoundLevel) : 'normal';
}

export const replaySound = $state<{ level: SoundLevel }>({ level: loadLevel() });

export function setReplaySoundLevel(level: SoundLevel) {
  replaySound.level = level;
  if (typeof localStorage !== 'undefined') {
    try {
      localStorage.setItem(LEVEL_KEY, level);
    } catch {
      // sin persistencia disponible
    }
  }
}

export function gainFor(level: SoundLevel): number {
  return SOUND_OPTIONS.find((o) => o.key === level)?.gain ?? 0.55;
}

export function soundLabel(level: SoundLevel): string {
  return t(`sound.${level}`);
}

// Suena por el mismo camino que el atajo (Rust), así se oye el sonido elegido, personalizado incluido.
export function playReplaySound() {
  invoke('test_save_sound').catch(() => {});
}
