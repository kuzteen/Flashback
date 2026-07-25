import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Clip } from './clips';
import type { MixerState } from './editor.svelte';

type SavedSegment = {
  start_ms: number;
  end_ms: number;
  pos_ms?: number | null;
  bound_start_ms?: number | null;
  bound_end_ms?: number | null;
  disabled?: boolean | null;
};

export type ShareEdit = { segments: SavedSegment[]; mixer: MixerState };

const MB = 1024 * 1024;

export const SIZE_PRESETS = [10, 50, 100] as const;

export const shareState = $state<{
  clip: Clip | null;
  edit: ShareEdit | null;
  watermark: boolean;
  preset: number | null;
  preparing: boolean;
  progress: number;
  error: string | null;
  dragging: boolean;
}>({
  clip: null,
  edit: null,
  watermark: false,
  preset: null,
  preparing: false,
  progress: 0,
  error: null,
  dragging: false
});

// Las rutas ya preparadas se recuerdan por preset mientras el diálogo está abierto, para que
// alternar entre tamaños no vuelva a esperar por algo que ya se recodificó.
let readyPaths = new Map<number | null, string>();

export function openShare(clip: Clip, edit: ShareEdit | null = null, watermark = false) {
  readyPaths = new Map();
  shareState.clip = clip;
  shareState.edit = edit;
  shareState.watermark = watermark;
  shareState.preset = null;
  shareState.preparing = false;
  shareState.progress = 0;
  shareState.error = null;
  shareState.dragging = false;
}

export function closeShare() {
  shareState.clip = null;
  shareState.edit = null;
  shareState.preset = null;
  shareState.preparing = false;
  shareState.error = null;
  shareState.dragging = false;
  readyPaths = new Map();
}

// Un preset que ya supera el tamaño del clip solo lo empeoraría: recodificar hacia arriba engorda
// el archivo y pierde calidad, así que se ofrece deshabilitado.
export function presetDisabled(clip: Clip | null, mb: number): boolean {
  return !!clip && clip.sizeBytes <= mb * MB;
}

export function selectPreset(mb: number | null) {
  if (shareState.preparing) return;
  shareState.preset = mb;
  shareState.error = null;
}

// La edición identidad se construye a partir de la duración del clip: el backend la reconoce y
// devuelve el archivo original sin recodificar nada.
function editFor(clip: Clip): ShareEdit {
  return (
    shareState.edit ?? {
      segments: [{ start_ms: 0, end_ms: clip.durationSec * 1000 }],
      mixer: { sys_vol: 1, sys_muted: false, mic_vol: 1, mic_muted: false }
    }
  );
}

async function prepare(clip: Clip, preset: number | null): Promise<string> {
  const cached = readyPaths.get(preset);
  if (cached) return cached;

  shareState.preparing = true;
  shareState.progress = 0;
  const unlisten = await listen<number>('share-progress', (e) => {
    shareState.progress = e.payload;
  });
  try {
    const path = await invoke<string>('share_prepare', {
      src: clip.path,
      edit: editFor(clip),
      targetBytes: preset === null ? null : preset * MB,
      watermark: shareState.watermark
    });
    readyPaths.set(preset, path);
    return path;
  } finally {
    unlisten();
    shareState.preparing = false;
    shareState.progress = 0;
  }
}

// El arrastre bloquea el hilo de UI de Windows mientras dura, así que esta promesa no resuelve
// hasta que el usuario suelta. Devuelve true solo si el destino aceptó el archivo.
export async function startDrag(): Promise<boolean> {
  const clip = shareState.clip;
  if (!clip || shareState.preparing || shareState.dragging) return false;
  shareState.error = null;
  try {
    const path = await prepare(clip, shareState.preset);
    shareState.dragging = true;
    return await invoke<boolean>('start_file_drag', { path });
  } catch (e) {
    shareState.error = String(e);
    console.error('share drag', e);
    return false;
  } finally {
    shareState.dragging = false;
  }
}
