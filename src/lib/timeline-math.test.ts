import { describe, expect, it } from 'vitest';
import type { Segment } from './edit-model';
import {
  clampZoom,
  contentWidth,
  extentMs,
  formatRulerLabel,
  formatTimecode,
  frameIndexAt,
  frameStepTarget,
  msPerPx,
  outStartOf,
  outToPos,
  posToOut,
  rulerStep,
  rulerTicks,
  zoomBy,
} from './timeline-math';

const seg = (startMs: number, endMs: number, posMs: number, extra: Partial<Segment> = {}): Segment => ({
  startMs,
  endMs,
  posMs,
  boundStartMs: startMs,
  boundEndMs: endMs,
  disabled: false,
  cropX: 0.5,
  cropY: 0.5,
  ...extra,
});

describe('zoom y escala', () => {
  it('limita el zoom y avanza por pasos multiplicativos', () => {
    expect(clampZoom(100)).toBe(40);
    expect(clampZoom(0.1)).toBe(0.5);
    expect(zoomBy(1, -1)).toBeCloseTo(1.15);
    expect(zoomBy(1, 1)).toBeCloseTo(1 / 1.15);
  });

  it('a zoom 1 el clip cabe justo; por debajo sobra sitio y por encima crece', () => {
    expect(contentWidth(1000, 1)).toBe(1000);
    expect(contentWidth(1000, 0.5)).toBe(1000);
    expect(contentWidth(1000, 4)).toBe(4000);
    expect(msPerPx(10_000, 1000, 1)).toBe(10);
    expect(msPerPx(10_000, 1000, 4)).toBe(2.5);
    expect(extentMs(10_000, 0.5)).toBe(20_000);
    expect(extentMs(10_000, 4)).toBe(10_000);
  });

  it('sin ancho no hay escala', () => {
    expect(msPerPx(10_000, 0, 1)).toBe(0);
  });
});

describe('salida ↔ posición', () => {
  const segs = [seg(0, 1000, 0), seg(2000, 3000, 1500)];

  it('outStartOf suma los bloques activos anteriores', () => {
    expect(outStartOf(segs, 1)).toBe(1000);
    expect(outStartOf([{ ...segs[0], disabled: true }, segs[1]], 1)).toBe(0);
  });

  it('outToPos coloca el tiempo de salida en la timeline', () => {
    expect(outToPos(segs, 500)).toBe(500);
    expect(outToPos(segs, 1200)).toBe(1700);
  });

  it('posToOut dentro de un bloque devuelve su tiempo de salida', () => {
    expect(posToOut(segs, 500)).toBe(500);
    expect(posToOut(segs, 1700)).toBe(1200);
  });

  it('posToOut en un hueco se engancha al borde activo más cercano', () => {
    expect(posToOut(segs, 1200)).toBe(1000);
    expect(posToOut(segs, 1450)).toBe(1000);
  });

  it('posToOut ignora los bloques desactivados', () => {
    expect(posToOut([{ ...segs[0], disabled: true }, segs[1]], 500)).toBe(0);
  });
});

describe('fotogramas', () => {
  const ft = [0, 33, 66, 100];

  it('frameIndexAt da el último fotograma que empieza antes', () => {
    expect(frameIndexAt(ft, 40)).toBe(1);
    expect(frameIndexAt(ft, 0)).toBe(0);
    expect(frameIndexAt(ft, -5)).toBe(-1);
  });

  it('frameStepTarget cae un poco dentro del fotograma contiguo', () => {
    expect(frameStepTarget(ft, 40, 1, 33)).toBeCloseTo(67.5);
    expect(frameStepTarget(ft, 40, -1, 33)).toBeCloseTo(1.5);
  });

  it('sin tabla no hay destino', () => {
    expect(frameStepTarget([], 40, 1, 33)).toBeNull();
  });
});

describe('regla', () => {
  it('elige el paso más fino que deja sitio a las etiquetas', () => {
    expect(rulerStep(10)).toBe(1000);
    expect(rulerStep(1)).toBe(100);
  });

  it('marca cinco subdivisiones por paso', () => {
    const ticks = rulerTicks(10_000, 10);
    expect(ticks).toHaveLength(51);
    expect(ticks.filter((t) => t.major)).toHaveLength(11);
    expect(ticks[1]).toEqual({ ms: 200, major: false });
  });

  it('sin escala no hay marcas', () => {
    expect(rulerTicks(10_000, 0)).toEqual([]);
  });
});

describe('formatos', () => {
  it('timecode con centésimas', () => {
    expect(formatTimecode(65_500)).toBe('01:05.50');
    expect(formatTimecode(-10)).toBe('00:00.00');
  });

  it('etiqueta de regla con décimas solo en pasos finos', () => {
    expect(formatRulerLabel(65_000, 1000)).toBe('1:05');
    expect(formatRulerLabel(1500, 500)).toBe('0:01.5');
  });
});
