import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Clip } from './clips';
import type { MixerState } from './editor-state.svelte';
import type { OutputFormat } from './edit-model';

type SavedSegment = {
  start_ms: number;
  end_ms: number;
  pos_ms?: number | null;
  bound_start_ms?: number | null;
  bound_end_ms?: number | null;
  disabled?: boolean | null;
};

export type ShareEdit = { segments: SavedSegment[]; mixer: MixerState; format?: OutputFormat };

const MB = 1024 * 1024;

// Debe coincidir con editor::CANCELLED en el backend.
export const CANCELLED = 'export-cancelled';

export const SIZE_PRESETS = [10, 50, 100] as const;

export const shareState = $state<{
  clip: Clip | null;
  edit: ShareEdit | null;
  // Duración de lo que realmente se comparte: con cortes activos no es la del clip de origen.
  durationSec: number;
  watermark: boolean;
  preset: number | null;
  preparing: boolean;
  progress: number;
  error: string | null;
  dragging: boolean;
}>({
  clip: null,
  edit: null,
  durationSec: 0,
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

// Identifica la petición en vuelo. El backend aborta el recodificado anterior al empezar otro, pero
// su promesa sigue viva y resuelve después: sin este testigo, una respuesta vieja pisaría el estado
// de la nueva selección.
let requestId = 0;

// El listener de progreso vive mientras el diálogo está abierto, no por preparación. Montarlo por
// petición metía un await antes del invoke, y dos clics seguidos podían llegar al backend en orden
// inverso: el trabajo viejo arrancaba el último y cancelaba al nuevo.
let unlistenProgress: (() => void) | null = null;
let sessionId = 0;

export function openShare(
  clip: Clip,
  edit: ShareEdit | null = null,
  watermark = false,
  durationSec?: number
) {
  requestId++;
  readyPaths = new Map();
  unlistenProgress?.();
  unlistenProgress = null;
  // El testigo de sesión descarta el listener de una apertura que ya quedó atrás: sin él, reabrir
  // el diálogo antes de que resolviera el listen dejaba el anterior vivo para siempre.
  const session = ++sessionId;
  listen<number>('share-progress', (e) => {
    if (shareState.preparing) shareState.progress = e.payload;
  }).then((un) => {
    if (sessionId === session && shareState.clip) unlistenProgress = un;
    else un();
  });
  shareState.clip = clip;
  shareState.edit = edit;
  shareState.durationSec = durationSec ?? clip.durationSec;
  shareState.watermark = watermark;
  shareState.preset = null;
  shareState.preparing = false;
  shareState.progress = 0;
  shareState.error = null;
  shareState.dragging = false;
  // Se prepara al abrir, no al arrastrar: desde el editor con cortes incluso "Original" hay que
  // materializarlo, y esperar al gesto dejaba al usuario tirando de algo que aún no existía. Sin
  // cortes ni preset el backend devuelve el archivo original y esto no cuesta nada.
  track(clip, null).catch(() => {});
}

export function closeShare() {
  requestId++;
  sessionId++;
  if (inFlight) invoke('share_cancel').catch(() => {});
  inFlight = null;
  unlistenProgress?.();
  unlistenProgress = null;
  shareState.clip = null;
  shareState.edit = null;
  shareState.preset = null;
  shareState.preparing = false;
  shareState.error = null;
  shareState.dragging = false;
  readyPaths = new Map();
}

// Peso aproximado de lo que se va a compartir. El recodificado conserva el bitrate del origen (hay
// un techo en él), así que el tamaño escala con la duración que se conserva: un recorte de 5 s de
// un clip de un minuto pesa una fracción, y ofrecerle el preset de 100 MB no tiene sentido.
export function sharedBytes(): number {
  const clip = shareState.clip;
  if (!clip) return 0;
  if (clip.durationSec <= 0) return clip.sizeBytes;
  return clip.sizeBytes * Math.min(1, shareState.durationSec / clip.durationSec);
}

// Un preset que ya supera el tamaño a compartir solo lo empeoraría: recodificar hacia arriba
// engorda el archivo y pierde calidad, así que se ofrece deshabilitado.
export function presetDisabled(mb: number): boolean {
  return !!shareState.clip && sharedBytes() <= mb * MB;
}

// Elegir un tamaño lo prepara al momento en vez de esperar al arrastre: cuando el usuario va a
// arrastrar, el archivo ya está. Se puede cambiar de tamaño o cancelar en cualquier momento; el
// backend aborta el recodificado anterior al arrancar el nuevo.
export function selectPreset(mb: number | null) {
  if (shareState.preset === mb && !shareState.error) return;
  shareState.preset = mb;
  shareState.error = null;
  const clip = shareState.clip;
  if (!clip) return;
  // Abandona el trabajo del preset anterior antes de pedir el nuevo. "Original" también se prepara:
  // desde el editor con cortes hay que materializarlo igual (si ya está en caché, es instantáneo).
  if (inFlight && inFlight.preset !== mb) cancelPrepare();
  track(clip, mb).catch(() => {});
}

// Una sola preparación por preset en vuelo: sin esto, arrastrar mientras se prepara arrancaría un
// segundo recodificado que abortaría al primero, y el usuario vería el progreso reiniciarse.
let inFlight: { preset: number | null; promise: Promise<string> } | null = null;

function track(clip: Clip, preset: number | null): Promise<string> {
  if (inFlight && inFlight.preset === preset) return inFlight.promise;
  const promise = prepare(clip, preset).finally(() => {
    if (inFlight?.promise === promise) inFlight = null;
  });
  inFlight = { preset, promise };
  return promise;
}

// `inFlight` y no `shareState.preparing`: ese flag se enciende con retardo, así que cancelar en los
// primeros milisegundos no habría abortado nada en el backend.
export function cancelPrepare() {
  requestId++;
  if (inFlight) invoke('share_cancel').catch(() => {});
  inFlight = null;
  shareState.preparing = false;
  shareState.progress = 0;
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

function prepare(clip: Clip, preset: number | null): Promise<string> {
  const cached = readyPaths.get(preset);
  if (cached) return Promise.resolve(cached);
  return runPrepare(clip, preset);
}

// Sin ningún await antes del invoke: así el backend recibe las peticiones en el orden en que el
// usuario pulsó, y cada begin_job aborta de verdad al anterior.
async function runPrepare(clip: Clip, preset: number | null): Promise<string> {
  const mine = ++requestId;
  const current = () => requestId === mine;
  shareState.progress = 0;
  // El velo de progreso solo aparece si la preparación de verdad tarda. El camino rápido (sin
  // cortes ni preset) resuelve en milisegundos y encenderlo siempre lo dejaba destellando.
  const veil = setTimeout(() => {
    if (current()) shareState.preparing = true;
  }, 150);
  try {
    const path = await invoke<string>('share_prepare', {
      src: clip.path,
      edit: editFor(clip),
      targetBytes: preset === null ? null : preset * MB,
      watermark: shareState.watermark
    });
    readyPaths.set(preset, path);
    return path;
  } catch (e) {
    // Un export abortado no es un fallo: lo pidió el usuario al cambiar de tamaño o cerrar.
    if (!String(e).includes(CANCELLED)) throw e;
    return '';
  } finally {
    clearTimeout(veil);
    if (current()) {
      shareState.preparing = false;
      shareState.progress = 0;
    }
  }
}

// El arrastre bloquea el hilo de UI de Windows mientras dura, así que esta promesa no resuelve
// hasta que el usuario suelta. Devuelve true solo si el destino aceptó el archivo.
export async function startDrag(): Promise<boolean> {
  const clip = shareState.clip;
  if (!clip || shareState.dragging) return false;
  shareState.error = null;
  try {
    const path = await track(clip, shareState.preset);
    // Vacío = la preparación se abortó (cambio de tamaño o cierre): no hay nada que arrastrar.
    if (!path || shareState.clip !== clip) return false;
    shareState.dragging = true;
    return await invoke<boolean>('start_file_drag', { path, thumbSrc: clip.path });
  } catch (e) {
    shareState.error = String(e);
    console.error('share drag', e);
    return false;
  } finally {
    shareState.dragging = false;
  }
}
