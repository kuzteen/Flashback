// Índice al que lleva una flecha del selector: sin dar la vuelta, así que en los extremos no se
// mueve (null). Un valor que no está en la lista entra por el extremo que toca.
export function stepIndex(count: number, index: number, dir: -1 | 1): number | null {
  if (count === 0) return null;
  if (index < 0) return dir > 0 ? 0 : count - 1;
  const next = index + dir;
  return next < 0 || next >= count ? null : next;
}
