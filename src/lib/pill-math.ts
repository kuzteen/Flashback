export type Span = { start: number; end: number };

export const LEAD = 0.2;
export const TRAIL = 0.34;

// Efecto oruga: el borde que va hacia la opción nueva sale primero y el otro lo alcanza después,
// así la pastilla se estira hacia la elección y luego se recoge.
export function pillTiming(from: Span | null, to: Span): Span {
  if (!from || (from.start === to.start && from.end === to.end)) return { start: 0, end: 0 };
  return to.start > from.start ? { start: TRAIL, end: LEAD } : { start: LEAD, end: TRAIL };
}
