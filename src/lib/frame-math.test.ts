import { describe, expect, it } from 'vitest';
import { contextInset, place, zoomOf } from './frame-math';

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

  it('zoom 2 dobla el recorte y la posición desplaza en los dos ejes', () => {
    close(place(1920, 1080, 1080, 1920, 2, 0.5, 0.5), [-2873.33, -960, 6826.67, 3840]);
    close(place(1920, 1080, 1080, 1920, 2, 0.5, 0), [-2873.33, 0, 6826.67, 3840]);
  });

  it('un origen ya vertical llena el lienzo con cualquier zoom', () => {
    close(place(1080, 1920, 1080, 1920, 0, 0.3, 0.8), [0, 0, 1080, 1920]);
    close(place(1080, 1920, 1080, 1920, 1, 0.3, 0.8), [0, 0, 1080, 1920]);
  });
});

describe('contextInset', () => {
  const frame = { w: 238, h: 422 };

  it('cuts the dimmed source at the same distance from the frame on both sides', () => {
    // Recuadro de 238px con 236px libres a cada lado; el vídeo sobresale 230 a la izquierda y 276
    // a la derecha: solo se recorta la derecha, lo que pasa de esos 236.
    const i = contextInset({ x: -230, y: 0, w: 744, h: 422 }, frame, 236);
    expect(i).toEqual({ top: 0, right: 40, bottom: 0, left: 0 });
  });

  it('cuts both sides when the source is wider than the room', () => {
    const i = contextInset({ x: -300, y: 0, w: 838, h: 422 }, frame, 236);
    expect(i).toEqual({ top: 0, right: 64, bottom: 0, left: 64 });
  });

  it('keeps the source to the frame height', () => {
    const i = contextInset({ x: -40, y: -30, w: 318, h: 482 }, frame, 236);
    expect(i).toEqual({ top: 30, right: 0, bottom: 30, left: 0 });
  });

  it('leaves a source inside the frame untouched', () => {
    expect(contextInset({ x: 20, y: 100, w: 198, h: 222 }, frame, 236)).toEqual({ top: 0, right: 0, bottom: 0, left: 0 });
  });
});
