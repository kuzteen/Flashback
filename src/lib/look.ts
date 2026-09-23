// Espejo exacto de src-tauri/src/look.rs: la vista previa aplica la misma matriz y el mismo núcleo
// que el export. Si cambia una, cambian las dos y sus tests.

export type Look = {
  brightness: number;
  contrast: number;
  saturation: number;
  temperature: number;
  sharpness: number;
};

export const NEUTRAL_LOOK: Look = { brightness: 0, contrast: 0, saturation: 0, temperature: 0, sharpness: 0 };

const EPS = 1e-4;
const LUMA = [0.2126, 0.7152, 0.0722];

const num = (v: unknown, lo: number, hi: number) =>
  typeof v === 'number' && Number.isFinite(v) ? Math.max(lo, Math.min(hi, v)) : 0;

export function readLook(l: Partial<Look> | null | undefined): Look {
  return {
    brightness: num(l?.brightness, -1, 1),
    contrast: num(l?.contrast, -1, 1),
    saturation: num(l?.saturation, -1, 1),
    temperature: num(l?.temperature, -1, 1),
    sharpness: num(l?.sharpness, 0, 1),
  };
}

export function hasColor(l: Look): boolean {
  return [l.brightness, l.contrast, l.saturation, l.temperature].some((v) => Math.abs(v) >= EPS);
}

export function isNeutral(l: Look): boolean {
  return !hasColor(l) && Math.abs(l.sharpness) < EPS;
}

// Filas = canales de salida (R, G, B); columnas = entrada R, G, B y desplazamiento.
export function colorMatrix(l: Look): number[][] {
  const temp = [1 + 0.1 * l.temperature, 1, 1 - 0.1 * l.temperature];
  const sat = 1 + l.saturation;
  const k = 1 + l.contrast;
  const offset = (1 - k) * 0.5 + 0.2 * l.brightness;
  return [0, 1, 2].map((i) => [
    ...[0, 1, 2].map((j) => k * ((1 - sat) * LUMA[j] + (i === j ? sat : 0)) * temp[j]),
    offset,
  ]);
}

export function sharpenKernel(l: Look): number[] | null {
  if (l.sharpness < EPS) return null;
  const a = 0.6 * l.sharpness;
  return [0, -a, 0, -a, 1 + 4 * a, -a, 0, -a, 0];
}

// feColorMatrix en orden SVG: 4 filas (R, G, B, A) de 5 valores; el alfa pasa intacto.
export function svgColorValues(l: Look): string {
  const m = colorMatrix(l);
  return [...m.map((r) => [r[0], r[1], r[2], 0, r[3]]), [0, 0, 0, 1, 0]].flat().map((v) => +v.toFixed(5)).join(' ');
}
