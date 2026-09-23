import type { OutputFormat } from './edit-model';

// Espejo exacto de la geometría de src-tauri/src/reframe.rs: la previsualización coloca el
// fotograma donde lo pondrá el export. Si cambia una, cambian las dos y sus tests.

export type Box = { x: number; y: number; w: number; h: number };

export const ZOOM_MAX = 2;

export function zoomOf(format: OutputFormat): number {
  if (format.kind !== 'vertical' || format.fill === 'fit') return 0;
  if (format.fill === 'crop') return 1;
  return Math.max(0, Math.min(ZOOM_MAX, format.zoom ?? 0.5));
}

// Dónde va el fotograma ENTERO dentro del lienzo (puede salirse). En cada eje, si el fotograma es
// mayor que el lienzo la posición desplaza qué parte se ve; si es menor, lo coloca en el lienzo.
export function place(sw: number, sh: number, ow: number, oh: number, zoom: number, cx: number, cy: number): Box {
  const fit = Math.min(ow / sw, oh / sh);
  const fill = Math.max(ow / sw, oh / sh);
  const z = Math.max(0, Math.min(ZOOM_MAX, zoom));
  const s = z <= 1 ? fit + (fill - fit) * z : fill * z;
  const w = sw * s;
  const h = sh * s;
  return { x: axis(w, ow, cx), y: axis(h, oh, cy), w, h };
}

function axis(size: number, canvas: number, pos: number): number {
  const p = Math.max(0, Math.min(1, pos));
  if (size > canvas) return Math.max(canvas - size, Math.min(0, canvas / 2 - p * size));
  return Math.max(0, Math.min(canvas - size, p * canvas - size / 2));
}
