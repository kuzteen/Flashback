import { invoke } from '@tauri-apps/api/core';

export type Filmstrip = {
  image: ImageBitmap;
  tileW: number;
  tileH: number;
  cols: number;
  timesMs: Float64Array;
};

// Solo en memoria y solo durante la sesión: ir y volver entre clips del editor es instantáneo
// sin dejar nada en disco. Pocas entradas porque cada imagen decodificada ocupa unos MB.
const MAX_CACHED = 6;
const cache = new Map<string, Promise<Filmstrip | null>>();

export const film = $state<{ path: string | null; strip: Filmstrip | null }>({ path: null, strip: null });

function parse(buf: ArrayBuffer): Promise<Filmstrip> {
  const v = new DataView(buf);
  const tileW = v.getUint32(0, true);
  const tileH = v.getUint32(4, true);
  const cols = v.getUint32(8, true);
  const n = v.getUint32(12, true);
  const timesMs = new Float64Array(n);
  for (let i = 0; i < n; i++) timesMs[i] = v.getFloat64(16 + i * 8, true);
  const jpeg = new Blob([buf.slice(16 + n * 8)], { type: 'image/jpeg' });
  return createImageBitmap(jpeg).then((image) => ({ image, tileW, tileH, cols, timesMs }));
}

function fetchStrip(path: string): Promise<Filmstrip | null> {
  let p = cache.get(path);
  if (p) {
    cache.delete(path);
    cache.set(path, p);
    return p;
  }
  p = invoke<ArrayBuffer>('clip_filmstrip', { path })
    .then(parse)
    .catch(() => null);
  cache.set(path, p);
  while (cache.size > MAX_CACHED) {
    const [oldest, old] = cache.entries().next().value!;
    cache.delete(oldest);
    void old.then((s) => {
      if (s && film.strip !== s) s.image.close();
    });
  }
  return p;
}

export function loadFilmstrip(path: string) {
  film.path = path;
  film.strip = null;
  void fetchStrip(path).then((s) => {
    if (film.path === path) film.strip = s;
  });
}

export function clearFilmstrip() {
  film.path = null;
  film.strip = null;
}

// Índice del fotograma más cercano a `ms` (tiempo de origen del clip).
export function tileAt(strip: Filmstrip, ms: number): number {
  const t = strip.timesMs;
  let lo = 0;
  let hi = t.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (t[mid] <= ms) lo = mid;
    else hi = mid - 1;
  }
  if (lo + 1 < t.length && t[lo + 1] - ms < ms - t[lo]) lo++;
  return lo;
}
