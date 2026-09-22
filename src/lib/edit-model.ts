// Estado editable del editor y sus operaciones. Todo es puro e inmutable: cada operación
// recibe segmentos y devuelve una lista nueva (o null si no procede), sin Svelte ni IPC, para
// que el historial pueda guardar copias y los tests no necesiten la app.

export const MIN_SEG_MS = 50;

export type Segment = {
  // Tramo de origen que se muestra.
  startMs: number;
  endMs: number;
  // Borde izquierdo en la línea de tiempo del editor; entre bloques puede haber huecos negros
  // que no se exportan.
  posMs: number;
  // Tamaño máximo del bloque: el material de fuera pertenece a otros bloques.
  boundStartMs: number;
  boundEndMs: number;
  disabled: boolean;
  // Centro horizontal del marco 9:16 en el formato vertical con recorte, de 0 a 1.
  cropX: number;
};

export type MixerState = {
  sys_vol: number;
  sys_muted: boolean;
  mic_vol: number;
  mic_muted: boolean;
};

export const DEFAULT_MIXER: MixerState = { sys_vol: 1, sys_muted: false, mic_vol: 1, mic_muted: false };

export type OutputFormat = { kind: 'horizontal' } | { kind: 'vertical'; fill: 'crop' | 'fit' };

export const DEFAULT_FORMAT: OutputFormat = { kind: 'horizontal' };

export type EditState = { segments: Segment[]; mixer: MixerState; format: OutputFormat };

export type SavedSegment = {
  start_ms: number;
  end_ms: number;
  pos_ms?: number | null;
  bound_start_ms?: number | null;
  bound_end_ms?: number | null;
  disabled?: boolean | null;
  crop_x?: number | null;
};

export type SavedEdit = {
  segments: SavedSegment[];
  mixer?: Partial<MixerState> | null;
  format?: OutputFormat | null;
};

const clamp01 = (x: number) => Math.max(0, Math.min(1, x));

export function segLen(s: Segment): number {
  return s.endMs - s.startMs;
}

export function fullClip(durationMs: number): Segment[] {
  return [
    { startMs: 0, endMs: durationMs, posMs: 0, boundStartMs: 0, boundEndMs: durationMs, disabled: false, cropX: 0.5 },
  ];
}

export function initialState(durationMs: number): EditState {
  return { segments: fullClip(durationMs), mixer: { ...DEFAULT_MIXER }, format: { ...DEFAULT_FORMAT } };
}

function readFormat(f: OutputFormat | null | undefined): OutputFormat {
  if (f?.kind === 'vertical' && (f.fill === 'crop' || f.fill === 'fit')) return { kind: 'vertical', fill: f.fill };
  return { kind: 'horizontal' };
}

// Ediciones guardadas por versiones anteriores no traen posición, límites, encuadre ni formato:
// se empaquetan en orden, su rango actual se toma como tamaño máximo y el encuadre va centrado.
export function fromSaved(saved: SavedEdit | null | undefined, durationMs: number): EditState {
  const base = initialState(durationMs);
  if (!saved) return base;
  const mixer = { ...DEFAULT_MIXER, ...(saved.mixer ?? {}) };
  const format = readFormat(saved.format);
  if (!saved.segments?.length) return { segments: base.segments, mixer, format };
  let acc = 0;
  const segments = saved.segments.map((s) => {
    const posMs = s.pos_ms ?? acc;
    acc = posMs + (s.end_ms - s.start_ms);
    return {
      startMs: s.start_ms,
      endMs: s.end_ms,
      posMs,
      boundStartMs: s.bound_start_ms ?? s.start_ms,
      boundEndMs: s.bound_end_ms ?? s.end_ms,
      disabled: s.disabled ?? false,
      cropX: clamp01(s.crop_x ?? 0.5),
    };
  });
  segments.sort((a, b) => a.posMs - b.posMs);
  return { segments, mixer, format };
}

export function toSaved(state: EditState, enabledOnly = false): SavedEdit {
  const list = enabledOnly ? state.segments.filter((s) => !s.disabled) : state.segments;
  return {
    segments: list.map((s) => ({
      start_ms: s.startMs,
      end_ms: s.endMs,
      pos_ms: s.posMs,
      bound_start_ms: s.boundStartMs,
      bound_end_ms: s.boundEndMs,
      disabled: s.disabled,
      crop_x: s.cropX,
    })),
    mixer: { ...state.mixer },
    format: state.format,
  };
}

export function keptMs(segs: Segment[]): number {
  return segs.reduce((a, s) => a + (s.disabled ? 0 : segLen(s)), 0);
}

// La salida concatena los bloques activos en orden, sin huecos. Más allá del final devuelve el
// final del último bloque activo, para que el cabezal nunca apunte a nada.
export function outToSeg(segs: Segment[], outMs: number): { index: number; srcMs: number } {
  let acc = 0;
  let last = -1;
  for (let i = 0; i < segs.length; i++) {
    const s = segs[i];
    if (s.disabled) continue;
    last = i;
    const d = segLen(s);
    if (outMs < acc + d) return { index: i, srcMs: s.startMs + Math.max(0, Math.min(outMs - acc, d)) };
    acc += d;
  }
  if (last >= 0) return { index: last, srcMs: segs[last].endMs };
  return { index: 0, srcMs: segs[0]?.startMs ?? 0 };
}

// Las listas son pequeñas: comparar por JSON es suficiente y evita un comparador a mano que
// habría que actualizar con cada campo nuevo.
export function equalState(a: EditState, b: EditState): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

// Las dos mitades quedan contiguas y cada una fija su propio rango como tamaño máximo: el
// material del otro lado del corte ya pertenece a la otra mitad.
export function cutAt(segs: Segment[], index: number, srcMs: number): Segment[] | null {
  const s = segs[index];
  if (!s) return null;
  if (srcMs - s.startMs < MIN_SEG_MS || s.endMs - srcMs < MIN_SEG_MS) return null;
  const left: Segment = { ...s, endMs: srcMs, boundStartMs: s.startMs, boundEndMs: srcMs };
  const right: Segment = {
    ...s,
    startMs: srcMs,
    posMs: s.posMs + (srcMs - s.startMs),
    boundStartMs: srcMs,
    boundEndMs: s.endMs,
  };
  return [...segs.slice(0, index), left, right, ...segs.slice(index + 1)];
}

// Acotado a [boundStartMs, boundEndMs] y a MIN_SEG_MS. Al recortar el inicio la posición se
// mueve con él para que el borde derecho no se desplace, y no puede crecer por encima del
// bloque anterior (la lista va ordenada por posición).
export function trim(segs: Segment[], index: number, edge: 'start' | 'end', srcMs: number): Segment[] {
  const s = segs[index];
  if (!s) return segs;
  const next = { ...s };
  if (edge === 'start') {
    const prev = segs[index - 1];
    const leftLimitPos = prev ? prev.posMs + segLen(prev) : 0;
    const minStartByPos = s.startMs + (leftLimitPos - s.posMs);
    const start = Math.max(s.boundStartMs, minStartByPos, Math.min(srcMs, s.endMs - MIN_SEG_MS));
    next.posMs = Math.max(0, s.posMs + (start - s.startMs));
    next.startMs = start;
  } else {
    next.endMs = Math.min(s.boundEndMs, Math.max(srcMs, s.startMs + MIN_SEG_MS));
  }
  return segs.map((x, i) => (i === index ? next : x));
}

export function removeAt(segs: Segment[], index: number): Segment[] | null {
  if (segs.length <= 1 || !segs[index]) return null;
  return segs.filter((_, i) => i !== index);
}

export function toggleDisabled(segs: Segment[], index: number): Segment[] {
  return segs.map((s, i) => (i === index ? { ...s, disabled: !s.disabled } : s));
}

export function setCrop(segs: Segment[], index: number, cropX: number): Segment[] {
  return segs.map((s, i) => (i === index ? { ...s, cropX: clamp01(cropX) } : s));
}

// La salida se reproduce y exporta en orden de lista: ordenar por posición hace que coincida con
// lo que se ve en la timeline.
export function sortByPos(segs: Segment[]): Segment[] {
  return [...segs].sort((a, b) => a.posMs - b.posMs);
}
