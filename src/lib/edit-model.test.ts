import { describe, expect, it } from 'vitest';
import {
  MIN_SEG_MS,
  cutAt,
  moveTo,
  removeAt,
  removeRange,
  snap,
  snapTargets,
  trimToPos,
  setCrop,
  sortByPos,
  toggleDisabled,
  trim,
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

describe('cutAt', () => {
  it('parte en dos bloques contiguos que fijan su propio rango como límite', () => {
    const out = cutAt([seg(0, 10_000, 0, { cropX: 0.2 })], 0, 4000);
    expect(out).toEqual([
      seg(0, 4000, 0, { cropX: 0.2 }),
      seg(4000, 10_000, 4000, { cropX: 0.2 }),
    ]);
  });

  it('no crea bloques más cortos que MIN_SEG_MS', () => {
    expect(cutAt(fullClip(10_000), 0, 30)).toBeNull();
    expect(cutAt(fullClip(10_000), 0, 9980)).toBeNull();
  });

  it('un índice inexistente no hace nada', () => {
    expect(cutAt(fullClip(10_000), 3, 4000)).toBeNull();
  });
});

describe('trim', () => {
  const cut = [seg(0, 4000, 0), seg(4000, 10_000, 4000)];

  it('el final no pasa del límite del bloque', () => {
    expect(trim(cut, 0, 'end', 3000)[0].endMs).toBe(3000);
    expect(trim(cut, 0, 'end', 5000)[0].endMs).toBe(4000);
  });

  it('el final respeta la longitud mínima', () => {
    expect(trim(cut, 0, 'end', 10)[0].endMs).toBe(MIN_SEG_MS);
  });

  it('recortar el inicio mueve también la posición para fijar el borde derecho', () => {
    const out = trim(cut, 1, 'start', 5000);
    expect(out[1].startMs).toBe(5000);
    expect(out[1].posMs).toBe(5000);
  });

  it('el inicio no crece por encima del bloque anterior', () => {
    const trimmed = trim(cut, 1, 'start', 5000);
    const back = trim(trimmed, 1, 'start', 0);
    expect(back[1].startMs).toBe(4000);
    expect(back[1].posMs).toBe(4000);
  });

  it('no modifica la lista recibida', () => {
    trim(cut, 0, 'end', 3000);
    expect(cut[0].endMs).toBe(4000);
  });
});

describe('removeAt / toggleDisabled / setCrop / sortByPos', () => {
  const two = [seg(0, 1000, 0), seg(1000, 2000, 1000)];

  it('quita un bloque pero nunca el último', () => {
    expect(removeAt(two, 0)).toEqual([seg(1000, 2000, 1000)]);
    expect(removeAt([seg(0, 1000, 0)], 0)).toBeNull();
  });

  it('alterna el desactivado', () => {
    expect(toggleDisabled(two, 1)[1].disabled).toBe(true);
    expect(toggleDisabled(toggleDisabled(two, 1), 1)[1].disabled).toBe(false);
  });

  it('limita el encuadre a [0, 1]', () => {
    expect(setCrop(two, 0, 1.4)[0].cropX).toBe(1);
    expect(setCrop(two, 0, -1)[0].cropX).toBe(0);
    expect(setCrop(two, 0, 0.25)[0].cropX).toBe(0.25);
  });

  it('ordena por posición en la timeline', () => {
    expect(sortByPos([two[1], two[0]])).toEqual(two);
  });
});

describe('snap', () => {
  it('se pega al objetivo más cercano dentro del umbral', () => {
    expect(snap(1040, [0, 1000, 2000], 50)).toEqual({ value: 1000, at: 1000 });
  });

  it('fuera del umbral no se mueve', () => {
    expect(snap(1100, [0, 1000, 2000], 50)).toEqual({ value: 1100, at: null });
  });

  it('con umbral 0 (Alt) queda desactivado', () => {
    expect(snap(1001, [1000], 0)).toEqual({ value: 1001, at: null });
  });
});

describe('snapTargets', () => {
  it('incluye inicio, cabezal y bordes de los demás bloques', () => {
    const segs = [seg(0, 1000, 0), seg(1000, 2000, 3000)];
    expect(snapTargets(segs, 1, 500).sort((a, b) => a - b)).toEqual([0, 0, 500, 1000]);
  });
});

describe('moveTo', () => {
  // A en [0, 1000], B de 1000 ms en 3000: el hueco libre para B es [1000, fin].
  const segs = [seg(0, 1000, 0), seg(1000, 2000, 3000)];

  it('coloca el bloque donde se pide si cabe', () => {
    const r = moveTo(segs, 1, 1500, 10_000, 0);
    expect(r.segments[1].posMs).toBe(1500);
    expect(r.snappedAt).toBeNull();
  });

  it('se pega al borde del hueco dentro del umbral', () => {
    const r = moveTo(segs, 1, 1080, 10_000, 100);
    expect(r.segments[1].posMs).toBe(1000);
    expect(r.snappedAt).toBe(1000);
  });

  it('puede pegar su borde derecho a un objetivo', () => {
    const r = moveTo(segs, 1, 1550, 10_000, 100, [2600]);
    expect(r.segments[1].posMs).toBe(1600);
    expect(r.snappedAt).toBe(2600);
  });

  it('nunca se solapa: salta al hueco libre más cercano', () => {
    const r = moveTo(segs, 0, 3200, 10_000, 0);
    expect(r.segments[0].posMs).toBe(4000);
  });
});

describe('trimToPos', () => {
  it('convierte la posición de timeline en tiempo de origen', () => {
    const moved = [seg(4000, 10_000, 6000)];
    expect(trimToPos(moved, 0, 'end', 9000)[0].endMs).toBe(7000);
    expect(trimToPos(moved, 0, 'start', 7000)[0].startMs).toBe(5000);
  });
});

describe('removeRange', () => {
  it('quita un tramo del medio y cierra el hueco', () => {
    expect(removeRange(fullClip(10_000), 2000, 5000)).toEqual([
      seg(0, 2000, 0),
      seg(5000, 10_000, 2000),
    ]);
  });

  it('abarca varios bloques', () => {
    const segs = [seg(0, 4000, 0), seg(4000, 10_000, 4000)];
    expect(removeRange(segs, 3000, 6000)).toEqual([seg(0, 3000, 0), seg(6000, 10_000, 3000)]);
  });

  it('desplaza a la izquierda lo que queda detrás aunque el rango caiga en un hueco', () => {
    const segs = [seg(0, 1000, 0), seg(1000, 2000, 5000)];
    expect(removeRange(segs, 2000, 3000)).toEqual([seg(0, 1000, 0), seg(1000, 2000, 4000)]);
  });

  it('descarta restos más cortos que MIN_SEG_MS', () => {
    expect(removeRange(fullClip(10_000), 30, 5000)).toEqual([seg(5000, 10_000, 30)]);
  });

  it('no deja la edición vacía', () => {
    expect(removeRange(fullClip(10_000), 0, 10_000)).toBeNull();
    expect(removeRange(fullClip(10_000), 0, 9980)).toBeNull();
  });

  it('un rango vacío no hace nada', () => {
    expect(removeRange(fullClip(10_000), 3000, 3000)).toBeNull();
  });
});
