import { cubicOut } from 'svelte/easing';

// Los avisos flotantes entran y salen deslizándose desde fuera de la ventana, sin fundido ni
// escala: escalar obliga a rasterizar de nuevo el contenido en cada fotograma y los trazos de los
// iconos bailaban. Solo mueve `transform`; el centrado va en `translate` para no perderlo.
// from: -1 baja desde arriba, 1 sube desde abajo.
export function toastSlide(_node: Element, { from, duration }: { from: 1 | -1; duration: number }) {
  const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
  return {
    duration: reduce ? 0 : duration,
    easing: cubicOut,
    css: (_t: number, u: number) => `transform: translateY(${u * 90 * from}px)`
  };
}

export const TOAST_IN = 420;
export const TOAST_OUT = 240;
