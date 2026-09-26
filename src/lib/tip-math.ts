export type TipSide = 'right' | 'bottom' | 'top';
export type Rect = { left: number; top: number; width: number; height: number };
export type Size = { w: number; h: number };
export type TipPlace = { x: number; y: number; caret: number };

const GAP = 10;
const EDGE = 8;
// La flecha no puede salir por las esquinas redondeadas del globo.
const CARET_MIN = 14;

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), Math.max(lo, hi));

// El globo se centra en el ancla y se mete dentro de la ventana; la flecha se desplaza lo mismo
// que el globo para seguir apuntando al ancla.
export function placeTip(anchor: Rect, size: Size, side: TipSide, view: Size): TipPlace {
  if (side === 'right') {
    const center = anchor.top + anchor.height / 2;
    const y = clamp(center - size.h / 2, EDGE, view.h - EDGE - size.h);
    return { x: anchor.left + anchor.width + GAP, y, caret: clamp(center - y, CARET_MIN, size.h - CARET_MIN) };
  }
  const center = anchor.left + anchor.width / 2;
  const x = clamp(center - size.w / 2, EDGE, view.w - EDGE - size.w);
  const y = side === 'top' ? anchor.top - GAP - size.h : anchor.top + anchor.height + GAP;
  return { x, y, caret: clamp(center - x, CARET_MIN, size.w - CARET_MIN) };
}

export function tipDirection(from: Rect, to: Rect, side: TipSide): 1 | -1 {
  const delta = side === 'right' ? to.top - from.top : to.left - from.left;
  return delta < 0 ? -1 : 1;
}

// Al pasar de un texto a otro que empieza igual ("…under 50 MB" → "…under 100 MB") solo se anima
// lo que cambia: animar la frase entera se leía como un parpadeo. Se corta en un espacio para no
// partir palabras.
export function splitSwap(from: string, to: string): { keep: string; swap: string } {
  let i = 0;
  while (i < from.length && i < to.length && from[i] === to[i]) i++;
  const cut = to.lastIndexOf(' ', i - 1) + 1;
  return { keep: to.slice(0, cut), swap: to.slice(cut) };
}
