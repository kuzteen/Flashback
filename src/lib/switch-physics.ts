export type Spring = { x: number; v: number };
export type SpringConfig = { k: number; c: number };

// Medidas del interruptor: borde de 1px y relleno de 2px alrededor de la bola. `stretched` es el
// ancho de la bola mientras se pulsa, inclinada hacia donde va a ir.
export const SIZES = {
  md: { w: 44, h: 25, inner: 38, knob: 19, stretched: 24 },
  sm: { w: 36, h: 20, inner: 30, knob: 14, stretched: 18 }
} as const;

// Mismo reparto que Motion para un muelle dado por duración visual y rebote: la duración fija la
// rigidez y el rebote cuánto amortigua.
export function springFor(visualDuration: number, bounce: number): SpringConfig {
  const root = (2 * Math.PI) / (visualDuration * 1.2);
  const damping = Math.min(Math.max(1 - bounce, 0.05), 1);
  return { k: root * root, c: 2 * damping * root };
}

const SUBSTEP = 1 / 240;
const REST_X = 0.001;
const REST_V = 0.01;

// Avanza el muelle dt segundos en pasos fijos (estable con muelles rígidos aunque un fotograma
// llegue tarde). Devuelve false cuando ya reposa en el destino.
export function step(s: Spring, to: number, { k, c }: SpringConfig, dt: number): boolean {
  let left = dt;
  while (left > 1e-9) {
    const h = Math.min(SUBSTEP, left);
    s.v += (-k * (s.x - to) - c * s.v) * h;
    s.x += s.v * h;
    left -= h;
  }
  if (Math.abs(s.x - to) < REST_X && Math.abs(s.v) < REST_V) {
    s.x = to;
    s.v = 0;
    return false;
  }
  return true;
}
