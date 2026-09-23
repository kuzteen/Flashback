import { describe, expect, it } from 'vitest';
import { NEUTRAL_LOOK, colorMatrix, isNeutral, readLook, sharpenKernel, svgColorValues } from './look';

function near(a: number[][], b: number[][]) {
  a.forEach((row, i) => row.forEach((v, j) => expect(v).toBeCloseTo(b[i][j], 4)));
}

describe('look', () => {
  it('neutro es la identidad y no necesita núcleo', () => {
    expect(isNeutral(NEUTRAL_LOOK)).toBe(true);
    near(colorMatrix(NEUTRAL_LOOK), [
      [1, 0, 0, 0],
      [0, 1, 0, 0],
      [0, 0, 1, 0],
    ]);
    expect(sharpenKernel(NEUTRAL_LOOK)).toBeNull();
  });

  it('brillo desplaza y contraste escala alrededor del gris medio', () => {
    near(colorMatrix({ ...NEUTRAL_LOOK, brightness: 0.5 }), [
      [1, 0, 0, 0.1],
      [0, 1, 0, 0.1],
      [0, 0, 1, 0.1],
    ]);
    near(colorMatrix({ ...NEUTRAL_LOOK, contrast: 0.5 }), [
      [1.5, 0, 0, -0.25],
      [0, 1.5, 0, -0.25],
      [0, 0, 1.5, -0.25],
    ]);
  });

  it('saturación -1 convierte cada canal en luminancia', () => {
    const luma = [0.2126, 0.7152, 0.0722, 0];
    near(colorMatrix({ ...NEUTRAL_LOOK, saturation: -1 }), [luma, luma, luma]);
  });

  it('temperatura cálida sube el rojo y baja el azul', () => {
    near(colorMatrix({ ...NEUTRAL_LOOK, temperature: 1 }), [
      [1.1, 0, 0, 0],
      [0, 1, 0, 0],
      [0, 0, 0.9, 0],
    ]);
  });

  it('la nitidez da un núcleo que respeta las zonas planas', () => {
    const k = sharpenKernel({ ...NEUTRAL_LOOK, sharpness: 1 })!;
    expect(k.reduce((a, b) => a + b, 0)).toBeCloseTo(1, 6);
    expect(k[4]).toBeCloseTo(3.4, 6);
  });

  it('limita los valores fuera de rango y descarta los inválidos', () => {
    expect(readLook({ brightness: 3, sharpness: -1, contrast: NaN })).toEqual({ ...NEUTRAL_LOOK, brightness: 1 });
    expect(readLook(null)).toEqual(NEUTRAL_LOOK);
  });

  it('la matriz SVG deja pasar el alfa', () => {
    expect(svgColorValues(NEUTRAL_LOOK)).toBe('1 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 0');
  });
});
