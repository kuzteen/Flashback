import { outToSeg, segLen, type Segment } from './edit-model';

// Geometría de la timeline, pura: la UI solo convierte píxeles del puntero con estas funciones.

export const ZOOM_MIN = 0.5;
export const ZOOM_MAX = 40;
export const SNAP_PX = 8;

export function clampZoom(z: number): number {
  return Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, z));
}

// Pasos multiplicativos: el zoom se siente igual de lejos que de cerca. Proporcional al giro: una
// muesca de rueda (100) es un 25 %, y un panel táctil, que manda deltas pequeños, avanza poco a poco.
export function zoomBy(zoom: number, deltaY: number): number {
  const d = Math.max(-300, Math.min(300, deltaY));
  return clampZoom(zoom * Math.pow(1.25, -d / 100));
}

// A zoom 1 el clip entero cabe en el ancho visible. Por debajo sobra sitio a la derecha para
// soltar bloques más allá del final; por encima el contenido crece y aparece scroll.
export function contentWidth(viewW: number, zoom: number): number {
  return viewW * Math.max(1, zoom);
}

export function msPerPx(totalMs: number, viewW: number, zoom: number): number {
  return viewW > 0 && zoom > 0 ? totalMs / (viewW * zoom) : 0;
}

export function extentMs(totalMs: number, zoom: number): number {
  return totalMs / Math.min(1, zoom);
}

// Donde acaba el último bloque: puede pasar del final del clip si se soltó ahí con el zoom
// alejado, y la línea de tiempo tiene que seguir llegando hasta él al acercarse.
export function blocksEnd(segs: Segment[]): number {
  return segs.reduce((m, s) => Math.max(m, s.posMs + segLen(s)), 0);
}

export function outStartOf(segs: Segment[], index: number): number {
  let acc = 0;
  for (let i = 0; i < index && i < segs.length; i++) if (!segs[i].disabled) acc += segLen(segs[i]);
  return acc;
}

export function outToPos(segs: Segment[], outMs: number): number {
  const { index, srcMs } = outToSeg(segs, outMs);
  const s = segs[index];
  return s ? s.posMs + (srcMs - s.startMs) : 0;
}

// Un punto dentro de un bloque activo da su tiempo de salida; en un hueco o sobre un bloque
// desactivado se engancha al borde activo más cercano, porque ahí no hay nada que mostrar.
export function posToOut(segs: Segment[], pos: number): number {
  let cum = 0;
  let bestT = 0;
  let bestDist = Infinity;
  for (const s of segs) {
    if (s.disabled) continue;
    const d = segLen(s);
    if (pos >= s.posMs && pos <= s.posMs + d) return cum + (pos - s.posMs);
    const dStart = Math.abs(pos - s.posMs);
    if (dStart < bestDist) {
      bestDist = dStart;
      bestT = cum;
    }
    const dEnd = Math.abs(pos - (s.posMs + d));
    if (dEnd < bestDist) {
      bestDist = dEnd;
      bestT = cum + d;
    }
    cum += d;
  }
  return bestT;
}

export function frameIndexAt(ft: number[], srcMs: number): number {
  let lo = 0;
  let hi = ft.length - 1;
  let ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (ft[mid] <= srcMs + 1e-3) {
      ans = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return ans;
}

// El clip es de framerate variable: se salta al fotograma contiguo de la tabla real y se cae un
// poco dentro de él (no en su borde) para que el decoder muestre justo ese y el paso sea
// constante. Sin tabla el llamante usa el paso aproximado por fps.
export function frameStepTarget(ft: number[], srcMs: number, dir: 1 | -1, frameMs: number): number | null {
  if (ft.length < 2) return null;
  const k = Math.max(0, frameIndexAt(ft, srcMs));
  const tk = Math.max(0, Math.min(ft.length - 1, k + dir));
  const next = tk + 1 < ft.length ? ft[tk + 1] : ft[tk] + frameMs;
  return ft[tk] + Math.min(1.5, (next - ft[tk]) * 0.25);
}

const STEPS = [100, 200, 500, 1000, 2000, 5000, 10_000, 15_000, 30_000, 60_000, 120_000, 300_000, 600_000];
const LABEL_MIN_PX = 72;

// pos: dónde va en la línea de tiempo; out: el tiempo que marca, el del vídeo exportado.
export type Tick = { pos: number; out: number; major: boolean };

export function rulerStep(mpp: number): number {
  for (const s of STEPS) if (s / mpp >= LABEL_MIN_PX) return s;
  return STEPS[STEPS.length - 1];
}

// La regla cuenta el tiempo del resultado, no la posición: solo hay marcas sobre los bloques
// activos, y cada una dice el segundo que tendrá en el vídeo exportado. En los huecos y sobre los
// bloques desactivados no hay nada, porque no se exportan. Los bloques van en orden de salida y
// los huecos solo separan, así que las etiquetas nunca quedan más juntas que el paso.
export function rulerTicks(segs: Segment[], mpp: number, from = -Infinity, to = Infinity): Tick[] {
  if (mpp <= 0) return [];
  const minor = rulerStep(mpp) / 5;
  const ticks: Tick[] = [];
  const active = segs.filter((s) => !s.disabled);
  let acc = 0;
  for (const [n, s] of active.entries()) {
    const len = segLen(s);
    // El segundo que cae justo en una unión es del bloque que empieza ahí; el final solo se marca
    // en el último, que no tiene siguiente.
    const end = n === active.length - 1 ? acc + len + 1e-6 : acc + len - 1e-6;
    for (let k = Math.max(0, Math.ceil(acc / minor - 1e-6)); k * minor <= end; k++) {
      const pos = s.posMs + k * minor - acc;
      if (pos > to) break;
      if (pos >= from) ticks.push({ pos, out: k * minor, major: k % 5 === 0 });
    }
    acc += len;
  }
  return ticks;
}

const two = (n: number) => String(Math.floor(n)).padStart(2, '0');

export function formatTimecode(ms: number): string {
  const sec = Math.max(0, ms / 1000) || 0;
  return `${two(sec / 60)}:${two(sec % 60)}.${two((sec * 100) % 100)}`;
}

export function formatRulerLabel(ms: number, step: number): string {
  const total = Math.round(ms);
  const m = Math.floor(total / 60_000);
  const s = Math.floor((total % 60_000) / 1000);
  const base = `${m}:${String(s).padStart(2, '0')}`;
  return step < 1000 ? `${base}.${Math.floor((total % 1000) / 100)}` : base;
}
