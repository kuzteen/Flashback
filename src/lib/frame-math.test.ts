import { describe, expect, it } from 'vitest';
import { place, zoomOf } from './frame-math';

// Mismos casos y cifras que los tests de src-tauri/src/reframe.rs: el visor tiene que colocar el
// fotograma exactamente donde lo pondrá el export.
const close = (a: { x: number; y: number; w: number; h: number }, b: [number, number, number, number]) => {
  expect(a.x).toBeCloseTo(b[0], 1);
  expect(a.y).toBeCloseTo(b[1], 1);
  expect(a.w).toBeCloseTo(b[2], 1);
  expect(a.h).toBeCloseTo(b[3], 1);
};

describe('zoomOf', () => {
  it('encajado y recorte son los extremos; personalizado usa su zoom', () => {
    expect(zoomOf({ kind: 'vertical', fill: 'fit', zoom: 0.7 })).toBe(0);
    expect(zoomOf({ kind: 'vertical', fill: 'crop', zoom: 0.7 })).toBe(1);
    expect(zoomOf({ kind: 'vertical', fill: 'custom', zoom: 0.7 })).toBe(0.7);
    expect(zoomOf({ kind: 'vertical', fill: 'custom' })).toBe(0.5);
    expect(zoomOf({ kind: 'horizontal' })).toBe(0);
  });
});

describe('place', () => {
  it('zoom 0 encaja el fotograma entero centrado', () => {
    close(place(1920, 1080, 1080, 1920, 0, 0.5, 0.5), [0, 656.25, 1080, 607.5]);
  });

  it('zoom 1 llena el lienzo y la posición horizontal desplaza lo que se ve', () => {
    close(place(1920, 1080, 1080, 1920, 1, 0.5, 0.5), [-1166.67, 0, 3413.33, 1920]);
    close(place(1920, 1080, 1080, 1920, 1, 0, 0.5), [0, 0, 3413.33, 1920]);
    close(place(1920, 1080, 1080, 1920, 1, 1, 0.5), [-2333.33, 0, 3413.33, 1920]);
  });

  it('un zoom intermedio recorta los lados y deja franjas arriba y abajo', () => {
    close(place(1920, 1080, 1080, 1920, 0.5, 0.5, 0.5), [-583.33, 328.13, 2246.67, 1263.75]);
  });

  it('la posición vertical mueve la franja dentro del lienzo', () => {
    close(place(1920, 1080, 1080, 1920, 0, 0.5, 0), [0, 0, 1080, 607.5]);
    close(place(1920, 1080, 1080, 1920, 0, 0.5, 1), [0, 1312.5, 1080, 607.5]);
  });

  it('un origen ya vertical llena el lienzo con cualquier zoom', () => {
    close(place(1080, 1920, 1080, 1920, 0, 0.3, 0.8), [0, 0, 1080, 1920]);
    close(place(1080, 1920, 1080, 1920, 1, 0.3, 0.8), [0, 0, 1080, 1920]);
  });
});
