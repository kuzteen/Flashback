import { describe, expect, it } from 'vitest';
import {
  DEFAULT_FORMAT,
  DEFAULT_MIXER,
  equalState,
  fromSaved,
  fullClip,
  initialState,
  keptMs,
  outToSeg,
  toSaved,
  type Segment,
} from './edit-model';

const seg = (startMs: number, endMs: number, posMs: number, extra: Partial<Segment> = {}): Segment => ({
  startMs,
  endMs,
  posMs,
  boundStartMs: startMs,
  boundEndMs: endMs,
  disabled: false,
  cropX: 0.5,
  ...extra,
});

describe('fullClip / initialState', () => {
  it('cubre el clip entero con un bloque centrado', () => {
    expect(fullClip(10_000)).toEqual([seg(0, 10_000, 0)]);
  });

  it('parte de mezcla y formato por defecto', () => {
    const s = initialState(10_000);
    expect(s.mixer).toEqual(DEFAULT_MIXER);
    expect(s.format).toEqual(DEFAULT_FORMAT);
  });
});

describe('fromSaved', () => {
  it('sin edición guardada devuelve el estado inicial', () => {
    expect(fromSaved(null, 10_000)).toEqual(initialState(10_000));
  });

  it('una edición antigua sin posición, límites, crop_x ni format se empaqueta en orden', () => {
    const s = fromSaved(
      {
        segments: [
          { start_ms: 0, end_ms: 1000 },
          { start_ms: 2000, end_ms: 3000 },
        ],
        mixer: { sys_vol: 0.5, sys_muted: false, mic_vol: 1, mic_muted: true },
      },
      10_000,
    );
    expect(s.segments).toEqual([seg(0, 1000, 0), seg(2000, 3000, 1000)]);
    expect(s.mixer).toEqual({ sys_vol: 0.5, sys_muted: false, mic_vol: 1, mic_muted: true });
    expect(s.format).toEqual({ kind: 'horizontal' });
  });

  it('ordena por posición y respeta límites, desactivado y encuadre guardados', () => {
    const s = fromSaved(
      {
        segments: [
          { start_ms: 5000, end_ms: 6000, pos_ms: 4000, bound_start_ms: 4000, bound_end_ms: 7000, disabled: true, crop_x: 0.2 },
          { start_ms: 0, end_ms: 1000, pos_ms: 0 },
        ],
      },
      10_000,
    );
    expect(s.segments).toEqual([
      seg(0, 1000, 0),
      seg(5000, 6000, 4000, { boundStartMs: 4000, boundEndMs: 7000, disabled: true, cropX: 0.2 }),
    ]);
  });

  it('limita crop_x a [0, 1]', () => {
    const s = fromSaved({ segments: [{ start_ms: 0, end_ms: 1000, crop_x: 2 }] }, 1000);
    expect(s.segments[0].cropX).toBe(1);
  });

  it('lee un formato vertical y descarta uno desconocido', () => {
    const v = fromSaved({ segments: [{ start_ms: 0, end_ms: 1000 }], format: { kind: 'vertical', fill: 'fit' } }, 1000);
    expect(v.format).toEqual({ kind: 'vertical', fill: 'fit' });
    const bad = fromSaved(
      { segments: [{ start_ms: 0, end_ms: 1000 }], format: { kind: 'square' } as never },
      1000,
    );
    expect(bad.format).toEqual({ kind: 'horizontal' });
  });

  it('una lista de segmentos vacía cae al clip completo', () => {
    expect(fromSaved({ segments: [] }, 5000).segments).toEqual(fullClip(5000));
  });
});

describe('toSaved', () => {
  it('serializa todo y vuelve a leerse igual', () => {
    const state = {
      segments: [seg(0, 1000, 0, { cropX: 0.3 }), seg(2000, 3000, 1500, { disabled: true })],
      mixer: DEFAULT_MIXER,
      format: { kind: 'vertical', fill: 'crop' } as const,
    };
    expect(fromSaved(toSaved(state), 10_000)).toEqual(state);
  });

  it('con enabledOnly omite los bloques desactivados', () => {
    const state = {
      segments: [seg(0, 1000, 0), seg(2000, 3000, 1000, { disabled: true })],
      mixer: DEFAULT_MIXER,
      format: DEFAULT_FORMAT,
    };
    expect(toSaved(state, true).segments).toHaveLength(1);
  });
});

describe('keptMs / outToSeg', () => {
  const segs = [seg(0, 1000, 0), seg(2000, 3000, 1000)];

  it('suma solo los bloques activos', () => {
    expect(keptMs(segs)).toBe(2000);
    expect(keptMs([segs[0], { ...segs[1], disabled: true }])).toBe(1000);
  });

  it('traduce tiempo de salida a bloque y tiempo de origen', () => {
    expect(outToSeg(segs, 500)).toEqual({ index: 0, srcMs: 500 });
    expect(outToSeg(segs, 1500)).toEqual({ index: 1, srcMs: 2500 });
  });

  it('más allá del final se queda en el final del último bloque activo', () => {
    expect(outToSeg(segs, 9000)).toEqual({ index: 1, srcMs: 3000 });
  });

  it('salta los bloques desactivados', () => {
    expect(outToSeg([{ ...segs[0], disabled: true }, segs[1]], 0)).toEqual({ index: 1, srcMs: 2000 });
  });
});

describe('equalState', () => {
  it('compara por contenido', () => {
    expect(equalState(initialState(1000), initialState(1000))).toBe(true);
    expect(equalState(initialState(1000), initialState(2000))).toBe(false);
  });
});
