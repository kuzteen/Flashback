import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import type { Clip } from './clips';

type RawClip = {
  id: string;
  name: string;
  path: string;
  size_bytes: number;
  modified_ms: number;
  duration_sec: number;
  source: string;
  detected: string;
  cover: string | null;
  cover_ms: number;
};

const FAV_KEY = 'flashback.favorites';

function loadFavs(): string[] {
  if (typeof localStorage === 'undefined') return [];
  try {
    const raw = localStorage.getItem(FAV_KEY);
    if (raw) return JSON.parse(raw);
  } catch {
    // localStorage corrupto o bloqueado
  }
  return [];
}

// Tarjetas grandes o lista compacta, en la biblioteca y en las playlists. Es preferencia de
// quien mira, no de cada lista: una sola, recordada en este equipo.
export type ClipView = 'cards' | 'list';
const VIEW_KEY = 'flashback.clips.view';

function loadView(): ClipView {
  try {
    return localStorage.getItem(VIEW_KEY) === 'list' ? 'list' : 'cards';
  } catch {
    return 'cards';
  }
}

export const clipView = $state<{ mode: ClipView }>({ mode: loadView() });

export function setClipView(mode: ClipView) {
  clipView.mode = mode;
  try {
    localStorage.setItem(VIEW_KEY, mode);
  } catch {}
}

export const library = $state<{ clips: Clip[]; loaded: boolean }>({ clips: [], loaded: false });

// Los favoritos no viven en el archivo: se guardan por id de clip (= nombre del MP4,
// estable) en localStorage. Cuando exista metadato real por clip se moverán al backend.
export const favorites = $state<{ ids: string[] }>({ ids: loadFavs() });

export function isFavorite(id: string): boolean {
  return favorites.ids.includes(id);
}

function persistFavs() {
  if (typeof localStorage !== 'undefined') {
    try {
      localStorage.setItem(FAV_KEY, JSON.stringify(favorites.ids));
    } catch {
      // sin persistencia disponible
    }
  }
}

export function toggleFavorite(id: string) {
  favorites.ids = isFavorite(id) ? favorites.ids.filter((x) => x !== id) : [...favorites.ids, id];
  persistFavs();
}

// Al renombrar/borrar un clip su id (= nombre del archivo) cambia o desaparece; se actualiza
// la lista de favoritos para que el estado siga al clip.
export function renameFavorite(oldId: string, newId: string) {
  if (!isFavorite(oldId)) return;
  favorites.ids = [...favorites.ids.filter((x) => x !== oldId), newId];
  persistFavs();
}

export function removeFavorite(id: string) {
  if (!isFavorite(id)) return;
  favorites.ids = favorites.ids.filter((x) => x !== id);
  persistFavs();
}

function toClip(r: RawClip, withEdits: Set<string>): Clip {
  return {
    id: r.id,
    title: r.name,
    source: r.source,
    detected: r.detected,
    coverSrc: r.cover ? `${convertFileSrc(r.cover)}?v=${r.cover_ms}` : null,
    durationSec: r.duration_sec,
    sizeBytes: r.size_bytes,
    createdAt: new Date(r.modified_ms),
    path: r.path,
    edited: withEdits.has(r.path),
    // Los clips exportados desde el editor se nombran `<nombre>_edit.mp4` o `.mov` según el formato
    // elegido (`_edit_2`… si se exporta más de una vez).
    exported: /_edit(_\d+)?\.(mp4|mov)$/i.test(r.id),
    // La fecha en la URL: un archivo nuevo con la ruta de otro no debe salir de la caché del
    // WebView como si fuera el anterior.
    previewSrc: `${convertFileSrc(r.path)}?v=${r.modified_ms}`
  };
}

// Miniaturas: el backend extrae un fotograma JPEG cacheado por clip. Se piden de forma
// perezosa (solo las tarjetas visibles) y con un límite de concurrencia para no saturar la
// generación al abrir una biblioteca grande. Una vez en disco, las siguientes peticiones
// devuelven al instante.
const thumbCache = new Map<string, string>();
const thumbStamp = new Map<string, number>();
const thumbQueue: (() => void)[] = [];
let thumbActive = 0;
const THUMB_CONCURRENCY = 4;

function pumpThumbs() {
  while (thumbActive < THUMB_CONCURRENCY && thumbQueue.length > 0) {
    const job = thumbQueue.shift();
    if (job) {
      thumbActive++;
      job();
    }
  }
}

// La rejilla virtualizada remonta tarjetas al reciclarlas; leer la caché de forma síncrona
// evita que una miniatura ya descargada parpadee contra el placeholder al volver a entrar.
export function cachedThumb(path: string): string | null {
  return thumbCache.get(path) ?? null;
}

export function requestThumb(path: string): Promise<string | null> {
  const cached = thumbCache.get(path);
  if (cached) return Promise.resolve(cached);
  return new Promise((resolve) => {
    thumbQueue.push(async () => {
      try {
        const p = await invoke<string>('clip_thumbnail', { path });
        const url = convertFileSrc(p);
        thumbCache.set(path, url);
        resolve(url);
      } catch {
        resolve(null);
      } finally {
        thumbActive--;
        pumpThumbs();
      }
    });
    pumpThumbs();
  });
}

export async function refreshLibrary() {
  try {
    const [raw, withEdits] = await Promise.all([
      invoke<RawClip[]>('list_clips'),
      invoke<string[]>('clips_with_edits').catch(() => [] as string[])
    ]);
    const edited = new Set(withEdits);
    for (const r of raw) {
      const was = thumbStamp.get(r.path);
      if (was !== undefined && was !== r.modified_ms) thumbCache.delete(r.path);
      thumbStamp.set(r.path, r.modified_ms);
    }
    library.clips = raw.map((r) => toClip(r, edited));
  } catch {
    // fuera de Tauri (preview en navegador): biblioteca vacía
    library.clips = [];
  }
  library.loaded = true;
}
