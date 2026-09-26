import { describe, expect, it } from 'vitest';
import { digitsOnly, parseSliderValue } from './slider-value';

describe('parseSliderValue', () => {
  it('reads whole numbers, negative ones too', () => {
    expect(parseSliderValue('35', -100, 100)).toBe(35);
    expect(parseSliderValue('-40', -100, 100)).toBe(-40);
  });

  it('ignores spaces, a percent sign and a leading plus', () => {
    expect(parseSliderValue(' 70 % ', 0, 200)).toBe(70);
    expect(parseSliderValue('+12', -100, 100)).toBe(12);
  });

  it('rounds decimals, with a dot or a comma', () => {
    expect(parseSliderValue('12.6', -100, 100)).toBe(13);
    expect(parseSliderValue('12,4', -100, 100)).toBe(12);
  });

  it('clamps to the slider range', () => {
    expect(parseSliderValue('250', -100, 100)).toBe(100);
    expect(parseSliderValue('-20', 0, 100)).toBe(0);
  });

  it('gives up on anything that is not a number', () => {
    expect(parseSliderValue('', -100, 100)).toBeNull();
    expect(parseSliderValue('-', -100, 100)).toBeNull();
    expect(parseSliderValue('abc', -100, 100)).toBeNull();
  });
});

describe('digitsOnly', () => {
  it('drops everything that is not a digit', () => {
    expect(digitsOnly('7a0%.5', true)).toBe('705');
  });

  it('keeps one leading minus only where negatives are allowed', () => {
    expect(digitsOnly('-40', true)).toBe('-40');
    expect(digitsOnly('4-0-', true)).toBe('40');
    expect(digitsOnly('--4', true)).toBe('-4');
    expect(digitsOnly('-40', false)).toBe('40');
  });
});
